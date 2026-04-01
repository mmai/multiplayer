use leptos::prelude::*;

use crate::app::GameUiState;
use crate::tic_tac_toe_logic::traits_implementation::GameState;
use super::cell::Cell;

#[component]
pub fn Board(state: GameUiState) -> impl IntoView {
    let game_over = state.game_state != GameState::Pending;
    let is_spectator = state.player_id > 1;
    // host (player 0) plays when next_move_host=true; guest (player 1) plays when false
    let is_my_turn = !game_over
        && !is_spectator
        && ((state.player_id == 0) == state.next_move_host);

    let cells: Vec<_> = (0..3_u8)
        .flat_map(|r| (0..3_u8).map(move |c| (r, c)))
        .map(|(row, col)| {
            let value = state.board[row as usize][col as usize];
            let clickable = is_my_turn && value == 0;
            view! { <Cell value=value row=row col=col clickable=clickable /> }
        })
        .collect();

    view! {
        <div class="board">
            {cells}
        </div>
    }
}
