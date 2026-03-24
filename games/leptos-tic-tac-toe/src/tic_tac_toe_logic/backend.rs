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

impl TicTacToeLogic {
    fn reset_game(&mut self) {
        self.command_list.push(BackendCommand::ResetViewState);
        self.view_state = ViewState::new(self.is_host_starting);
    }
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
        if !self.allow_spectators && (player > 1) {
            self.command_list
                .push(BackendCommand::KickPlayer { player });
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
        let delta = ViewStateDelta {
            is_circle: (player_id == 0),
            column: payload.column,
            row: payload.row,
        };
        self.view_state.apply_delta(&delta);
        self.command_list.push(BackendCommand::Delta(delta));
        if self.view_state.game_state != GameState::Pending {
            self.command_list.push(BackendCommand::SetTimer {
                timer_id: 0,
                duration: 5.0,
            });
        }
    }

    fn timer_triggered(&mut self, _: u16) {
        self.is_host_starting = !self.is_host_starting;
        self.reset_game();
    }

    fn get_view_state(&self) -> &ViewState {
        &self.view_state
    }

    fn drain_commands(&mut self) -> Vec<BackendCommand<ViewStateDelta>> {
        std::mem::take(&mut self.command_list)
    }
}
