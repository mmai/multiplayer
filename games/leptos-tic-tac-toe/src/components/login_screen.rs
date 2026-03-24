use futures::channel::mpsc::UnboundedSender;
use leptos::prelude::*;

use crate::app::NetCommand;

#[component]
pub fn LoginScreen(error: Option<String>) -> impl IntoView {
    let (room_name, set_room_name) = signal(String::new());
    let (allow_spectators, set_allow_spectators) = signal(false);

    let cmd_tx = use_context::<UnboundedSender<NetCommand>>()
        .expect("UnboundedSender<NetCommand> not found in context");

    // Each button needs its own clone of the sender.
    let cmd_tx_create = cmd_tx.clone();
    let cmd_tx_join = cmd_tx;

    view! {
        <div class="login-container">
            <h1>"Tic-Tac-Toe"</h1>

            {error.map(|err| view! { <p class="error-msg">{err}</p> })}

            <input
                type="text"
                placeholder="Room name"
                prop:value=move || room_name.get()
                on:input=move |ev| set_room_name.set(event_target_value(&ev))
            />

            <label>
                <input
                    type="checkbox"
                    prop:checked=move || allow_spectators.get()
                    on:change=move |ev| set_allow_spectators.set(event_target_checked(&ev))
                />
                " Allow spectators"
            </label>

            <button
                class="btn btn-primary"
                disabled=move || room_name.get().is_empty()
                on:click=move |_| {
                    cmd_tx_create
                        .unbounded_send(NetCommand::CreateRoom {
                            room: room_name.get(),
                            allow_spectators: allow_spectators.get(),
                        })
                        .ok();
                }
            >
                "Create Room"
            </button>

            <button
                class="btn btn-secondary"
                disabled=move || room_name.get().is_empty()
                on:click=move |_| {
                    cmd_tx_join
                        .unbounded_send(NetCommand::JoinRoom { room: room_name.get() })
                        .ok();
                }
            >
                "Join Room"
            </button>
        </div>
    }
}
