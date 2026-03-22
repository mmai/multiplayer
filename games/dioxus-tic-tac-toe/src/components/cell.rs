use dioxus::prelude::*;
use crate::app::NetCommand;

#[component]
pub fn Cell(value: u8, row: u8, col: u8, clickable: bool) -> Element {
    let net = use_context::<Coroutine<NetCommand>>();

    let (symbol, extra_class) = match value {
        1 => ("X", " cross"),
        2 => ("O", " circle"),
        _ => ("", ""),
    };

    let class = if clickable {
        format!("cell{extra_class} clickable")
    } else {
        format!("cell{extra_class}")
    };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                if clickable {
                    net.send(NetCommand::PlaceStone { column: col, row });
                }
            },
            "{symbol}"
        }
    }
}
