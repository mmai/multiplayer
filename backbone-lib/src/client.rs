//! Background task for the client (non-host) side of a session.

use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::platform::sleep_ms;
use crate::protocol::{parse_client_update, send_disconnect, send_rpc};
use crate::session::{BackendMsg, SessionEvent};
use crate::traits::SerializationCap;

pub(crate) async fn client_loop<A, D, VS>(
    mut ws_sender: WsSender,
    ws_receiver: WsReceiver,
    mut action_rx: UnboundedReceiver<BackendMsg<A>>,
    event_tx: UnboundedSender<SessionEvent<D, VS>>,
) where
    A: SerializationCap,
    D: SerializationCap,
    VS: SerializationCap,
{
    loop {
        // 1. Drain outbound actions.
        loop {
            match action_rx.try_next() {
                Ok(Some(BackendMsg::Action(action))) => {
                    send_rpc(&mut ws_sender, &action);
                }
                Ok(Some(BackendMsg::Disconnect)) => {
                    send_disconnect(&mut ws_sender, false);
                    event_tx
                        .unbounded_send(SessionEvent::Disconnected(None))
                        .ok();
                    return;
                }
                Ok(None) => {
                    send_disconnect(&mut ws_sender, false);
                    return;
                }
                Err(_) => break,
            }
        }

        // 2. Drain inbound state updates.
        loop {
            match ws_receiver.try_recv() {
                Some(WsEvent::Message(WsMessage::Binary(data))) => {
                    match parse_client_update::<VS, D>(data) {
                        Ok(updates) => {
                            for u in updates {
                                event_tx
                                    .unbounded_send(SessionEvent::Update(u))
                                    .ok();
                            }
                        }
                        Err(e) => {
                            event_tx
                                .unbounded_send(SessionEvent::Disconnected(Some(e)))
                                .ok();
                            return;
                        }
                    }
                }
                Some(WsEvent::Closed) => {
                    event_tx
                        .unbounded_send(SessionEvent::Disconnected(Some(
                            "Connection closed".to_string(),
                        )))
                        .ok();
                    return;
                }
                Some(WsEvent::Error(e)) => {
                    event_tx
                        .unbounded_send(SessionEvent::Disconnected(Some(e)))
                        .ok();
                    return;
                }
                Some(_) => continue,
                None => break,
            }
        }

        sleep_ms(2).await;
    }
}
