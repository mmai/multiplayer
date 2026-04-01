//! The public-facing session API.
//!
//! # Usage
//!
//! ```ignore
//! // Connect (async, returns after handshake completes)
//! let mut session: GameSession<MyAction, MyDelta, MyState> =
//!     GameSession::connect::<MyBackend>(RoomConfig {
//!         relay_url: "ws://localhost:8080/ws".to_string(),
//!         game_id: "my-game".to_string(),
//!         room_id: "room-42".to_string(),
//!         rule_variation: 0,
//!         role: RoomRole::Create,
//!         reconnect_token: None,
//!     })
//!     .await?;
//!
//! // In a loop (e.g. Dioxus coroutine with futures::select!):
//! loop {
//!     futures::select! {
//!         cmd = ui_rx.next().fuse() => session.send_action(cmd),
//!         event = session.next_event().fuse() => match event {
//!             Some(SessionEvent::Update(u)) => view_state.apply(u),
//!             Some(SessionEvent::Disconnected(reason)) | None => break,
//!         }
//!     }
//! }
//! ```

use ewebsock::{WsEvent, WsMessage};
use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use protocol::JoinRequest;

use crate::client::client_loop;
use crate::host::host_loop;
use crate::platform::{TaskBound, sleep_ms, spawn_task};
use crate::protocol::{parse_handshake_response, send_join_request};
use crate::traits::{BackEndArchitecture, SerializationCap, ViewStateUpdate};

// ---------------------------------------------------------------------------
// Public configuration types
// ---------------------------------------------------------------------------

/// Whether to create a new room (host) or join an existing one (client).
pub enum RoomRole {
    Create,
    Join,
}

/// Configuration required to connect to a game session.
pub struct RoomConfig {
    /// WebSocket URL of the relay server (e.g. `"ws://localhost:8080/ws"`).
    pub relay_url: String,
    /// Game identifier registered on the relay (e.g. `"tic-tac-toe"`).
    pub game_id: String,
    /// Room identifier shared between host and clients.
    pub room_id: String,
    /// Game mode/variant. Only used when `role` is `Create`.
    pub rule_variation: u16,
    pub role: RoomRole,
    /// If `Some`, attempt to reconnect to an existing session instead of creating/joining fresh.
    /// The value is the token returned by a previous successful handshake.
    pub reconnect_token: Option<u64>,
    /// Serialized backend state for host reconnect.
    ///
    /// Produced by the app layer (e.g. `serde_json::to_vec(&view_state)`) and stored in
    /// localStorage. Passed to [`BackEndArchitecture::from_bytes`] when the host
    /// reconnects so the game can resume from the last known state.
    /// Ignored for non-host reconnects and normal connections.
    pub host_state: Option<Vec<u8>>,
}

/// Error returned by [`GameSession::connect`].
#[derive(Debug)]
pub enum ConnectError {
    WebSocket(String),
    Handshake(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::WebSocket(e) => write!(f, "WebSocket error: {e}"),
            ConnectError::Handshake(e) => write!(f, "Handshake error: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal message type (UI → background task)
// ---------------------------------------------------------------------------

pub(crate) enum BackendMsg<A> {
    Action(A),
    Disconnect,
}

// ---------------------------------------------------------------------------
// Session event (background task → UI)
// ---------------------------------------------------------------------------

/// Events emitted by the session to the UI.
pub enum SessionEvent<Delta, ViewState> {
    /// A state update arrived from the host backend.
    Update(ViewStateUpdate<ViewState, Delta>),
    /// The session ended. `None` = clean disconnect, `Some(reason)` = error.
    Disconnected(Option<String>),
}

// ---------------------------------------------------------------------------
// GameSession
// ---------------------------------------------------------------------------

/// A connected game session.
///
/// Created by [`GameSession::connect`]. Holds channels to the background task
/// that owns the WebSocket connection and (on host) the game backend.
pub struct GameSession<Action, Delta, ViewState> {
    /// The player ID assigned by the relay server. Always `0` for the host.
    pub player_id: u16,
    /// The game mode/variant selected by the host.
    pub rule_variation: u16,
    /// `true` if this client is hosting the game (runs the backend).
    pub is_host: bool,
    /// Token to persist in localStorage for reconnect on page refresh.
    /// Only meaningful for non-host players (player_id > 0).
    pub reconnect_token: u64,
    action_tx: UnboundedSender<BackendMsg<Action>>,
    event_rx: UnboundedReceiver<SessionEvent<Delta, ViewState>>,
}

impl<A, D, VS> GameSession<A, D, VS>
where
    A: SerializationCap + TaskBound,
    D: SerializationCap + Clone + TaskBound,
    VS: SerializationCap + Clone + TaskBound,
{
    /// Connects to the relay server and performs the handshake.
    ///
    /// Returns after the relay confirms the player ID and rule variation.
    /// Spawns a background task that drives the WebSocket connection for the
    /// lifetime of the session.
    ///
    /// # Errors
    /// Returns `Err` if the WebSocket cannot be opened or the handshake fails.
    pub async fn connect<Backend>(config: RoomConfig) -> Result<Self, ConnectError>
    where
        Backend: BackEndArchitecture<A, D, VS> + TaskBound,
    {
        let create_room = matches!(config.role, RoomRole::Create);

        // 1. Open WebSocket.
        let (mut ws_sender, ws_receiver) =
            ewebsock::connect(&config.relay_url, ewebsock::Options::default())
                .map_err(|e| ConnectError::WebSocket(e.to_string()))?;

        // 2. Wait for the Opened event (WASM WebSocket is async).
        loop {
            match ws_receiver.try_recv() {
                Some(WsEvent::Opened) => break,
                Some(WsEvent::Error(e)) => return Err(ConnectError::WebSocket(e)),
                Some(WsEvent::Closed) => {
                    return Err(ConnectError::WebSocket("Connection closed".to_string()));
                }
                Some(_) => continue,
                None => sleep_ms(1).await,
            }
        }

        // 3. Send the join request.
        let req = JoinRequest {
            game_id: config.game_id,
            room_id: config.room_id,
            rule_variation: config.rule_variation,
            create_room,
            reconnect_token: config.reconnect_token,
        };
        send_join_request(&mut ws_sender, &req).map_err(ConnectError::Handshake)?;

        // 4. Wait for the handshake response.
        let (player_id, rule_variation, reconnect_token) = loop {
            match ws_receiver.try_recv() {
                Some(WsEvent::Message(WsMessage::Binary(data))) => {
                    break parse_handshake_response(data).map_err(ConnectError::Handshake)?;
                }
                Some(WsEvent::Error(e)) => return Err(ConnectError::Handshake(e)),
                Some(WsEvent::Closed) => {
                    // The relay may have sent a binary error frame just before
                    // closing. ewebsock can deliver Closed before that frame,
                    // so drain one more message to catch it.
                    if let Some(WsEvent::Message(WsMessage::Binary(data))) =
                        ws_receiver.try_recv()
                    {
                        break parse_handshake_response(data)
                            .map_err(ConnectError::Handshake)?;
                    }
                    return Err(ConnectError::Handshake(
                        "Connection closed during handshake".to_string(),
                    ));
                }
                Some(_) => continue,
                None => sleep_ms(1).await,
            }
        };

        // The relay assigns player_id == 0 exclusively to the host.
        let is_host = player_id == 0;

        // 5. Set up channels between the UI and the background task.
        let (action_tx, action_rx) = mpsc::unbounded::<BackendMsg<A>>();
        let (event_tx, event_rx) = mpsc::unbounded::<SessionEvent<D, VS>>();

        // 6. Spawn the background event loop.
        if is_host {
            spawn_task(host_loop::<A, D, VS, Backend>(
                ws_sender,
                ws_receiver,
                action_rx,
                event_tx,
                rule_variation,
                config.host_state,
            ));
        } else {
            spawn_task(client_loop::<A, D, VS>(
                ws_sender,
                ws_receiver,
                action_rx,
                event_tx,
            ));
        }

        Ok(GameSession {
            player_id,
            rule_variation,
            is_host,
            reconnect_token,
            action_tx,
            event_rx,
        })
    }

    /// Sends a game action to the backend (fire-and-forget).
    pub fn send_action(&self, action: A) {
        self.action_tx
            .unbounded_send(BackendMsg::Action(action))
            .ok();
    }

    /// Awaits the next session event.
    ///
    /// Returns `None` if the background task has exited (i.e. the session is
    /// over). Normal termination arrives as `Some(SessionEvent::Disconnected(_))`
    /// before the channel closes.
    pub async fn next_event(&mut self) -> Option<SessionEvent<D, VS>> {
        self.event_rx.next().await
    }

    /// Signals the background task to send a graceful disconnect message and
    /// shut down. Consumes the session.
    pub fn disconnect(self) {
        self.action_tx
            .unbounded_send(BackendMsg::Disconnect)
            .ok();
    }
}
