use backbone_lib::transport_layer::{ConnectionState, TransportLayer, ViewStateUpdate};
use dioxus::prelude::*;
use futures_util::{FutureExt, StreamExt};

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
        let mut transport: TransportLayer<
            StonePlacement,
            ViewStateDelta,
            TicTacToeLogic,
            ViewState,
        > = TransportLayer::generate_transport_layer(
            "ws://127.0.0.1:8080/ws".to_string(),
            "tic-tac-toe".to_string(),
        );

        let mut local_view_state: Option<ViewState> = None;

        loop {
            // Drain all commands queued by UI events since last tick.
            while let Some(cmd) = rx.next().now_or_never().flatten() {
                match cmd {
                    NetCommand::CreateRoom { room, allow_spectators } => {
                        transport.start_game_server(room, if allow_spectators { 1 } else { 0 });
                    }
                    NetCommand::JoinRoom { room } => {
                        transport.start_game_client(room);
                    }
                    NetCommand::PlaceStone { column, row } => {
                        transport.register_server_rpc(StonePlacement { column, row });
                    }
                }
            }

            // Advance the transport state machine (~16 ms tick).
            transport.update(0.016);

            // Reflect the new connection state into the screen signal.
            match transport.connection_state().clone() {
                ConnectionState::Disconnected { error_string } => {
                    local_view_state = None;
                    screen.set(Screen::Login { error: error_string });
                }
                ConnectionState::AwaitingHandshake | ConnectionState::ExecutingHandshake => {
                    screen.set(Screen::Connecting);
                }
                ConnectionState::Connected { player_id, .. } => {
                    if local_view_state.is_none() {
                        local_view_state = Some(ViewState::new(true));
                    }
                    let vs = local_view_state.as_mut().unwrap();

                    // Drain all pending view updates before rendering.
                    while let Some(update) = transport.get_next_update() {
                        match update {
                            ViewStateUpdate::Full(state) => *vs = state,
                            ViewStateUpdate::Incremental(delta) => vs.apply_delta(&delta),
                        }
                    }

                    screen.set(Screen::Playing(GameUiState {
                        board: vs.board.clone(),
                        game_state: vs.game_state.clone(),
                        next_move_host: vs.next_move_host,
                        player_id,
                    }));
                }
            }

            // Sleep ~16 ms before the next tick (~60 fps polling).
            gloo_timers::future::TimeoutFuture::new(16).await;
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
