use leptos::prelude::*;

use crate::app::GameUiState;
use crate::tic_tac_toe_logic::traits_implementation::GameState;
use super::board::Board;

fn status_text(state: &GameUiState) -> &'static str {
    match &state.game_state {
        GameState::CrossWins => "X wins!",
        GameState::CircleWins => "O wins!",
        GameState::Draw => "Draw!",
        GameState::Pending => {
            if state.player_id > 1 {
                "Spectating"
            } else if (state.player_id == 0) == state.next_move_host {
                "Your turn"
            } else {
                "Opponent's turn"
            }
        }
    }
}

#[component]
pub fn GameScreen(state: GameUiState) -> impl IntoView {
    let status = status_text(&state);
    view! {
        <div class="game-container">
            <p class="status-bar">{status}</p>
            <Board state=state />
        </div>
    }
}
