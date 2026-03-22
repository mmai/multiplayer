use dioxus::prelude::*;
use crate::app::NetCommand;

#[component]
pub fn LoginScreen(error: Option<String>) -> Element {
    let mut room_name = use_signal(String::new);
    let mut allow_spectators = use_signal(|| false);
    let net = use_context::<Coroutine<NetCommand>>();

    rsx! {
        div { class: "login-container",
            h1 { "Tic-Tac-Toe" }

            if let Some(err) = error {
                p { class: "error-msg", "{err}" }
            }

            input {
                r#type: "text",
                placeholder: "Room name",
                value: "{room_name}",
                oninput: move |evt| room_name.set(evt.value()),
            }

            label {
                input {
                    r#type: "checkbox",
                    checked: allow_spectators(),
                    onchange: move |evt| allow_spectators.set(evt.checked()),
                }
                " Allow spectators"
            }

            button {
                class: "btn btn-primary",
                disabled: room_name().is_empty(),
                onclick: move |_| {
                    net.send(NetCommand::CreateRoom {
                        room: room_name(),
                        allow_spectators: allow_spectators(),
                    });
                },
                "Create Room"
            }

            button {
                class: "btn btn-secondary",
                disabled: room_name().is_empty(),
                onclick: move |_| {
                    net.send(NetCommand::JoinRoom { room: room_name() });
                },
                "Join Room"
            }
        }
    }
}
