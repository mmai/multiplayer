use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ViewStateDelta {
    pub is_circle: bool,
    pub column: u8,
    pub row: u8,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StonePlacement {
    pub column: u8,
    pub row: u8,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ViewState {
    pub board: Vec<Vec<u8>>,
    pub next_move_host: bool,
    pub game_state: GameState,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub enum GameState {
    Pending,
    CrossWins,
    CircleWins,
    Draw,
}

impl ViewState {
    pub fn new(is_host_starting: bool) -> ViewState {
        let mut board = Vec::with_capacity(3);
        for _ in 0..3 {
            board.push(vec![0_u8, 0_u8, 0_u8]);
        }
        ViewState {
            board,
            game_state: GameState::Pending,
            next_move_host: is_host_starting,
        }
    }

    pub fn apply_delta(&mut self, delta: &ViewStateDelta) {
        self.board[delta.row as usize][delta.column as usize] = if delta.is_circle { 2 } else { 1 };
        self.next_move_host = !self.next_move_host;
        self.game_state = self.check_winning();
    }

    pub fn check_legality(&self, move_data: &StonePlacement, player_id: u16) -> bool {
        if player_id > 1 {
            return false;
        }
        if (player_id == 0) != self.next_move_host {
            return false;
        }
        self.board[move_data.row as usize][move_data.column as usize] == 0
    }

    fn check_for(&self, probe: u8) -> bool {
        (0..3).any(|row| (0..3).all(|col| self.board[row][col] == probe))
            || (0..3).any(|col| (0..3).all(|row| self.board[row][col] == probe))
            || (0..3).all(|i| self.board[i][i] == probe)
            || (0..3).all(|i| self.board[i][2 - i] == probe)
    }

    pub fn check_winning(&self) -> GameState {
        if self.check_for(1) {
            return GameState::CrossWins;
        }
        if self.check_for(2) {
            return GameState::CircleWins;
        }
        if self.board.iter().flatten().all(|x| *x != 0) {
            return GameState::Draw;
        }
        GameState::Pending
    }
}
