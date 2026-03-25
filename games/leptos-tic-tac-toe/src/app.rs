use futures::channel::mpsc;
use futures::{FutureExt, StreamExt};
use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

use backbone_lib::session::{ConnectError, GameSession, RoomConfig, RoomRole, SessionEvent};
use backbone_lib::traits::ViewStateUpdate;

use crate::components::{ConnectingScreen, GameScreen, LoginScreen};
use crate::tic_tac_toe_logic::backend::TicTacToeLogic;
use crate::tic_tac_toe_logic::traits_implementation::{
    StonePlacement, ViewState, ViewStateDelta, GameState,
};

const RELAY_URL: &str = "ws://127.0.0.1:8080/ws";
const GAME_ID: &str = "tic-tac-toe";
const STORAGE_KEY: &str = "ttt_session";

/// The complete game state the UI needs to render the board.
#[derive(Clone, PartialEq)]
pub struct GameUiState {
    pub board: Vec<Vec<u8>>,
    pub game_state: GameState,
    /// true = host's turn (host plays ○, value 2)
    pub next_move_host: bool,
    /// 0 = host (○), 1 = guest (✕), >1 = spectator
    pub player_id: u16,
}

/// Which screen the app is currently showing.
#[derive(Clone, PartialEq)]
pub enum Screen {
    Login { error: Option<String> },
    Connecting,
    Playing(GameUiState),
}

/// Commands sent from UI event handlers into the network task.
pub enum NetCommand {
    CreateRoom { room: String, allow_spectators: bool },
    JoinRoom { room: String },
    /// Attempt to rejoin a previous session using a stored token.
    Reconnect { relay_url: String, game_id: String, room_id: String, token: u64 },
    PlaceStone { column: u8, row: u8 },
}

/// Stored in localStorage to enable reconnecting after a page refresh.
#[derive(Serialize, Deserialize)]
struct StoredSession {
    relay_url: String,
    game_id: String,
    room_id: String,
    token: u64,
}

fn save_session(session: &StoredSession) {
    LocalStorage::set(STORAGE_KEY, session).ok();
}

fn load_session() -> Option<StoredSession> {
    LocalStorage::get::<StoredSession>(STORAGE_KEY).ok()
}

fn clear_session() {
    LocalStorage::delete(STORAGE_KEY);
}

#[component]
pub fn App() -> impl IntoView {
    let stored = load_session();
    let initial_screen = if stored.is_some() {
        Screen::Connecting
    } else {
        Screen::Login { error: None }
    };
    let screen = RwSignal::new(initial_screen);

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded::<NetCommand>();
    // Provide the sender so child components can dispatch commands.
    provide_context(cmd_tx.clone());

    // If there is a stored session, queue a reconnect attempt before spawning the task.
    if let Some(s) = stored {
        cmd_tx
            .unbounded_send(NetCommand::Reconnect {
                relay_url: s.relay_url,
                game_id: s.game_id,
                room_id: s.room_id,
                token: s.token,
            })
            .ok();
    }

    spawn_local(async move {
        loop {
            // Wait for a create / join / reconnect command from the UI.
            let (config, is_reconnect) = loop {
                match cmd_rx.next().await {
                    Some(NetCommand::CreateRoom { room, allow_spectators }) => {
                        break (
                            RoomConfig {
                                relay_url: RELAY_URL.to_string(),
                                game_id: GAME_ID.to_string(),
                                room_id: room,
                                rule_variation: u16::from(allow_spectators),
                                role: RoomRole::Create,
                                reconnect_token: None,
                            },
                            false,
                        );
                    }
                    Some(NetCommand::JoinRoom { room }) => {
                        break (
                            RoomConfig {
                                relay_url: RELAY_URL.to_string(),
                                game_id: GAME_ID.to_string(),
                                room_id: room,
                                rule_variation: 0,
                                role: RoomRole::Join,
                                reconnect_token: None,
                            },
                            false,
                        );
                    }
                    Some(NetCommand::Reconnect { relay_url, game_id, room_id, token }) => {
                        break (
                            RoomConfig {
                                relay_url,
                                game_id,
                                room_id,
                                rule_variation: 0,
                                role: RoomRole::Join,
                                reconnect_token: Some(token),
                            },
                            true,
                        );
                    }
                    _ => {} // Ignore game commands while not connected.
                }
            };

            screen.set(Screen::Connecting);

            let room_id_for_storage = config.room_id.clone();
            let mut session: GameSession<StonePlacement, ViewStateDelta, ViewState> =
                match GameSession::connect::<TicTacToeLogic>(config).await {
                    Ok(s) => s,
                    Err(ConnectError::WebSocket(e) | ConnectError::Handshake(e)) => {
                        if is_reconnect {
                            // The stored session is no longer valid.
                            clear_session();
                        }
                        screen.set(Screen::Login { error: Some(e) });
                        continue;
                    }
                };

            // Persist session for non-host players so they can reconnect on refresh.
            if !session.is_host {
                save_session(&StoredSession {
                    relay_url: RELAY_URL.to_string(),
                    game_id: GAME_ID.to_string(),
                    room_id: room_id_for_storage,
                    token: session.reconnect_token,
                });
            }

            let player_id = session.player_id;
            let mut vs = ViewState::new(session.is_host);

            // Run the game loop until disconnected.
            loop {
                futures::select! {
                    cmd = cmd_rx.next().fuse() => match cmd {
                        Some(NetCommand::PlaceStone { column, row }) => {
                            session.send_action(StonePlacement { column, row });
                        }
                        _ => {
                            // Any other command (or channel close) while playing
                            // means the user wants to leave intentionally.
                            clear_session();
                            session.disconnect();
                            screen.set(Screen::Login { error: None });
                            break;
                        }
                    },
                    event = session.next_event().fuse() => match event {
                        Some(SessionEvent::Update(u)) => {
                            match u {
                                ViewStateUpdate::Full(state) => vs = state,
                                ViewStateUpdate::Incremental(delta) => vs.apply_delta(&delta),
                            }
                            screen.set(Screen::Playing(GameUiState {
                                board: vs.board.clone(),
                                game_state: vs.game_state.clone(),
                                next_move_host: vs.next_move_host,
                                player_id,
                            }));
                        }
                        Some(SessionEvent::Disconnected(reason)) => {
                            screen.set(Screen::Login { error: reason });
                            break;
                        }
                        None => {
                            screen.set(Screen::Login { error: None });
                            break;
                        }
                    }
                }
            }
        }
    });

    view! {
        {move || match screen.get() {
            Screen::Login { error } => view! { <LoginScreen error=error /> }.into_any(),
            Screen::Connecting => view! { <ConnectingScreen /> }.into_any(),
            Screen::Playing(state) => view! { <GameScreen state=state /> }.into_any(),
        }}
    }
}
