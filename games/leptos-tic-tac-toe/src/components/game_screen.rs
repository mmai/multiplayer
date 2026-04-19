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
    let auth_username = use_context::<RwSignal<Option<String>>>()
        .expect("auth_username not found in context");

    view! {
        <div class="game-container">
            <p class="status-bar">{status}</p>
            <Board state=state />
            {move || auth_username.get().map(|u| view! {
                <p class="playing-as">"Playing as " <strong>{u}</strong></p>
            })}
        </div>
    }
}
