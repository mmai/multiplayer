use dioxus::prelude::*;
use crate::app::GameUiState;
use crate::tic_tac_toe_logic::traits_implementation::GameState;
use super::cell::Cell;

#[component]
pub fn Board(state: GameUiState) -> Element {
    let game_over = state.game_state != GameState::Pending;
    let is_spectator = state.player_id > 1;
    // host (player 0) plays when next_move_host=true; guest (player 1) plays when false
    let is_my_turn = !game_over
        && !is_spectator
        && ((state.player_id == 0) == state.next_move_host);

    rsx! {
        div { class: "board",
            for (row, col) in (0..3_u8).flat_map(|r| (0..3_u8).map(move |c| (r, c))) {
                {
                    let is_grayed = state.grayed_square == Some((row, col));
                    let empty = state.board[row as usize][col as usize] == 0;
                    rsx! {
                        Cell {
                            key: "{row}-{col}",
                            value: state.board[row as usize][col as usize],
                            row,
                            col,
                            clickable: is_my_turn && empty && !is_grayed,
                            grayed: is_grayed,
                        }
                    }
                }
            }
        }
    }
}
