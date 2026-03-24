use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub enum ViewStateDelta {
    StonePlaced { is_circle: bool, column: u8, row: u8 },
    SquareGrayed { row: u8, col: u8 },
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
    /// The square that is grayed out this turn (row, col). `None` before the
    /// first random result arrives.
    pub grayed_square: Option<(u8, u8)>,
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
            grayed_square: None,
        }
    }

    pub fn apply_delta(&mut self, delta: &ViewStateDelta) {
        match delta {
            ViewStateDelta::StonePlaced { is_circle, column, row } => {
                self.board[*row as usize][*column as usize] = if *is_circle { 2 } else { 1 };
                self.next_move_host = !self.next_move_host;
                self.game_state = self.check_winning();
                self.grayed_square = None;
            }
            ViewStateDelta::SquareGrayed { row, col } => {
                self.grayed_square = Some((*row, *col));
                if self.game_state == GameState::Pending && !self.has_available_moves() {
                    self.game_state = GameState::Draw;
                }
            }
        }
    }

    pub fn check_legality(&self, move_data: &StonePlacement, player_id: u16) -> bool {
        if player_id > 1 {
            return false;
        }
        if (player_id == 0) != self.next_move_host {
            return false;
        }
        if self.board[move_data.row as usize][move_data.column as usize] != 0 {
            return false;
        }
        self.grayed_square != Some((move_data.row, move_data.column))
    }

    /// Returns true if there is at least one cell that is both empty and not grayed.
    pub fn has_available_moves(&self) -> bool {
        (0..3usize).any(|row| {
            (0..3usize).any(|col| {
                self.board[row][col] == 0
                    && self.grayed_square != Some((row as u8, col as u8))
            })
        })
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
