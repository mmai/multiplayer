mod app;
mod components;
mod tic_tac_toe_logic;

use app::App;
use leptos::prelude::*;

fn main() {
    mount_to_body(|| view! { <App /> })
}
