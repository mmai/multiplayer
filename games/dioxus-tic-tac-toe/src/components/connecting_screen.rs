use dioxus::prelude::*;

#[component]
pub fn ConnectingScreen() -> Element {
    rsx! {
        p { class: "connecting", "Connecting…" }
    }
}
