use backbone_lib::session::{ConnectError, GameSession, RoomConfig, RoomRole, SessionEvent};
use backbone_lib::traits::ViewStateUpdate;
use dioxus::prelude::*;
use futures::{FutureExt, StreamExt};

const STYLE: Asset = asset!("/assets/style.css");

use crate::components::{ConnectingScreen, GameScreen, LoginScreen};
use crate::tic_tac_toe_logic::backend::TicTacToeLogic;
use crate::tic_tac_toe_logic::traits_implementation::{
    GameState, StonePlacement, ViewState, ViewStateDelta,
};

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

/// Commands sent from UI events into the network coroutine.
pub enum NetCommand {
    CreateRoom { room: String, allow_spectators: bool },
    JoinRoom { room: String },
    PlaceStone { column: u8, row: u8 },
}

#[component]
pub fn App() -> Element {
    let mut screen: Signal<Screen> = use_signal(|| Screen::Login { error: None });
    provide_context(screen);

    let net = use_coroutine(move |mut rx: UnboundedReceiver<NetCommand>| async move {
        loop {
            // Wait for a create/join command from the login screen.
            let config = loop {
                match rx.next().await {
                    Some(NetCommand::CreateRoom { room, allow_spectators }) => {
                        break RoomConfig {
                            relay_url: "ws://127.0.0.1:8080/ws".to_string(),
                            game_id: "tic-tac-toe".to_string(),
                            room_id: room,
                            rule_variation: u16::from(allow_spectators),
                            role: RoomRole::Create,
                        };
                    }
                    Some(NetCommand::JoinRoom { room }) => {
                        break RoomConfig {
                            relay_url: "ws://127.0.0.1:8080/ws".to_string(),
                            game_id: "tic-tac-toe".to_string(),
                            room_id: room,
                            rule_variation: 0,
                            role: RoomRole::Join,
                        };
                    }
                    _ => {} // Ignore game commands while not connected.
                }
            };

            screen.set(Screen::Connecting);

            let mut session: GameSession<StonePlacement, ViewStateDelta, ViewState> =
                match GameSession::connect::<TicTacToeLogic>(config).await {
                    Ok(s) => s,
                    Err(ConnectError::WebSocket(e) | ConnectError::Handshake(e)) => {
                        screen.set(Screen::Login { error: Some(e) });
                        continue;
                    }
                };

            let player_id = session.player_id;
            let mut vs = ViewState::new(session.is_host);

            // Run the game loop until disconnected.
            loop {
                futures::select! {
                    cmd = rx.next().fuse() => match cmd {
                        Some(NetCommand::PlaceStone { column, row }) => {
                            session.send_action(StonePlacement { column, row });
                        }
                        _ => {
                            // Any other command (or channel close) while playing
                            // means the user wants to leave.
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

    provide_context(net);

    rsx! {
        document::Stylesheet { href: STYLE }
        match screen() {
            Screen::Login { error } => rsx! { LoginScreen { error } },
            Screen::Connecting => rsx! { ConnectingScreen {} },
            Screen::Playing(state) => rsx! { GameScreen { state } },
        }
    }
}
