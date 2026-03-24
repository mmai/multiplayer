use crate::tic_tac_toe_logic::traits_implementation::{
    GameState, StonePlacement, ViewState, ViewStateDelta,
};
use backbone_lib::traits::{BackEndArchitecture, BackendCommand};

pub struct TicTacToeLogic {
    command_list: Vec<BackendCommand<ViewStateDelta>>,
    view_state: ViewState,
    is_host_starting: bool,
    allow_spectators: bool,
}

impl BackEndArchitecture<StonePlacement, ViewStateDelta, ViewState> for TicTacToeLogic {
    fn new(rule_variation: u16) -> Self {
        TicTacToeLogic {
            is_host_starting: true,
            command_list: Vec::new(),
            view_state: ViewState::new(true),
            allow_spectators: rule_variation == 1,
        }
    }

    fn player_arrival(&mut self, player: u16) {
        if !self.allow_spectators && player > 1 {
            self.command_list
                .push(BackendCommand::KickPlayer { player });
            return;
        }
        if player == 1 {
            // Second player joined: request the initial grayed square. Sent after
            // send_full_state so RANDOM_RESULT arrives after the client is synced.
            self.command_list
                .push(BackendCommand::RequestRandom { request_id: 0 });
        }
    }

    fn player_departure(&mut self, player: u16) {
        if player == 1 {
            self.command_list.push(BackendCommand::TerminateRoom);
        }
    }

    fn inform_rpc(&mut self, player_id: u16, payload: StonePlacement) {
        if self.view_state.game_state != GameState::Pending {
            return;
        }
        if !self.view_state.check_legality(&payload, player_id) {
            return;
        }
        let delta = ViewStateDelta::StonePlaced {
            is_circle: player_id == 0,
            column: payload.column,
            row: payload.row,
        };
        self.view_state.apply_delta(&delta);
        self.command_list.push(BackendCommand::Delta(delta));
        if self.view_state.game_state == GameState::Pending {
            // Game continues: request a new grayed square for the next turn.
            self.command_list
                .push(BackendCommand::RequestRandom { request_id: 0 });
        } else {
            // Game over: schedule reset.
            self.command_list.push(BackendCommand::SetTimer {
                timer_id: 0,
                duration: 5.0,
            });
        }
    }

    fn random_result(&mut self, _request_id: u16, value: u64) {
        if self.view_state.game_state != GameState::Pending {
            return;
        }
        let row = ((value / 3) % 3) as u8;
        let col = (value % 3) as u8;
        // Apply to backend state so check_legality can use it.
        // Do NOT emit Delta: clients receive RANDOM_RESULT directly from the relay
        // and apply the grayed square independently, making it tamper-evident.
        self.view_state.apply_delta(&ViewStateDelta::SquareGrayed { row, col });
        if self.view_state.game_state != GameState::Pending {
            self.command_list.push(BackendCommand::SetTimer {
                timer_id: 0,
                duration: 5.0,
            });
        }
    }

    fn timer_triggered(&mut self, _: u16) {
        self.is_host_starting = !self.is_host_starting;
        self.view_state = ViewState::new(self.is_host_starting);
        self.command_list.push(BackendCommand::ResetViewState);
        // Request a grayed square for the new game.
        self.command_list
            .push(BackendCommand::RequestRandom { request_id: 0 });
    }

    fn get_view_state(&self) -> &ViewState {
        &self.view_state
    }

    fn drain_commands(&mut self) -> Vec<BackendCommand<ViewStateDelta>> {
        std::mem::take(&mut self.command_list)
    }
}
