use futures::channel::mpsc::UnboundedSender;
use leptos::prelude::*;

use crate::app::NetCommand;

#[component]
pub fn Cell(value: u8, row: u8, col: u8, clickable: bool) -> impl IntoView {
    let cmd_tx = use_context::<UnboundedSender<NetCommand>>()
        .expect("UnboundedSender<NetCommand> not found in context");

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

    view! {
        <div
            class=class
            on:click=move |_| {
                if clickable {
                    cmd_tx
                        .unbounded_send(NetCommand::PlaceStone { column: col, row })
                        .ok();
                }
            }
        >
            {symbol}
        </div>
    }
}
