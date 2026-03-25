//! Background task for the host (game server) side of a session.

use std::collections::HashSet;

use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use web_time::{Duration, Instant};

use crate::platform::sleep_ms;
use crate::protocol::{
    ToServerCommand, parse_server_command, send_delta, send_disconnect, send_full_state,
    send_kick, send_reset,
};
use crate::session::{BackendMsg, SessionEvent};
use crate::traits::{BackEndArchitecture, BackendCommand, SerializationCap, ViewStateUpdate};

struct Timer {
    id: u16,
    fire_at: Instant,
}

pub(crate) async fn host_loop<A, D, VS, Backend>(
    mut ws_sender: WsSender,
    ws_receiver: WsReceiver,
    mut action_rx: UnboundedReceiver<BackendMsg<A>>,
    event_tx: UnboundedSender<SessionEvent<D, VS>>,
    rule_variation: u16,
    host_state: Option<Vec<u8>>,
) where
    A: SerializationCap,
    D: SerializationCap + Clone,
    VS: SerializationCap + Clone,
    Backend: BackEndArchitecture<A, D, VS>,
{
    let mut backend = host_state
        .as_deref()
        .and_then(|b| Backend::from_bytes(rule_variation, b))
        .unwrap_or_else(|| Backend::new(rule_variation));
    backend.player_arrival(0);

    // Push initial state to UI immediately.
    let initial = backend.get_view_state().clone();
    event_tx
        .unbounded_send(SessionEvent::Update(ViewStateUpdate::Full(initial)))
        .ok();

    let mut timers: Vec<Timer> = Vec::new();
    let mut cancelled_timers: HashSet<u16> = HashSet::new();
    let mut remote_player_count: u16 = 0;

    loop {
        let mut client_joined = false;

        // 1. Drain local actions / detect session drop or disconnect request.
        loop {
            match action_rx.try_next() {
                Ok(Some(BackendMsg::Action(action))) => {
                    backend.inform_rpc(0, action);
                }
                Ok(Some(BackendMsg::Disconnect)) => {
                    send_disconnect(&mut ws_sender, true);
                    event_tx
                        .unbounded_send(SessionEvent::Disconnected(None))
                        .ok();
                    return;
                }
                Ok(None) => {
                    // All senders dropped — session was dropped without calling disconnect().
                    send_disconnect(&mut ws_sender, true);
                    return;
                }
                Err(_) => break, // Channel empty; nothing pending.
            }
        }

        // 2. Drain WebSocket events from the relay.
        loop {
            match ws_receiver.try_recv() {
                Some(WsEvent::Message(WsMessage::Binary(data))) => {
                    match parse_server_command::<A>(data) {
                        ToServerCommand::ClientJoin(id) => {
                            backend.player_arrival(id);
                            remote_player_count += 1;
                            client_joined = true;
                        }
                        ToServerCommand::ClientLeft(id) => {
                            backend.player_departure(id);
                            remote_player_count = remote_player_count.saturating_sub(1);
                        }
                        ToServerCommand::Rpc(id, payload) => {
                            backend.inform_rpc(id, payload);
                        }
                        ToServerCommand::Error(e) => {
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
                Some(_) => continue, // Ignore Opened / text messages.
                None => break,       // No more events this iteration.
            }
        }

        // 3. Fire elapsed timers.
        let now = Instant::now();
        let mut fired = Vec::new();
        timers.retain(|t| {
            if t.fire_at <= now {
                fired.push(t.id);
                false
            } else {
                true
            }
        });
        for id in fired {
            if !cancelled_timers.remove(&id) {
                backend.timer_triggered(id);
            }
        }

        // 4. Drain and process backend commands.
        let commands = backend.drain_commands();

        if commands.is_empty() && !client_joined {
            sleep_ms(2).await;
            continue;
        }

        let mut delta_batch: Vec<D> = Vec::new();
        let mut reset = false;

        for cmd in commands {
            match cmd {
                BackendCommand::TerminateRoom => {
                    send_disconnect(&mut ws_sender, true);
                    event_tx
                        .unbounded_send(SessionEvent::Disconnected(None))
                        .ok();
                    return;
                }
                BackendCommand::SetTimer { timer_id, duration } => {
                    // Cancel any existing timer with the same id, then re-arm.
                    timers.retain(|t| t.id != timer_id);
                    cancelled_timers.remove(&timer_id);
                    timers.push(Timer {
                        id: timer_id,
                        fire_at: Instant::now() + Duration::from_secs_f32(duration),
                    });
                }
                BackendCommand::CancelTimer { timer_id } => {
                    cancelled_timers.insert(timer_id);
                }
                BackendCommand::KickPlayer { player } => {
                    if remote_player_count > 0 {
                        send_kick(&mut ws_sender, player);
                    }
                }
                BackendCommand::ResetViewState => {
                    reset = true;
                }
                BackendCommand::Delta(d) => {
                    delta_batch.push(d);
                }
            }
        }

        if reset {
            // Reset supersedes all pending deltas: send fresh full state.
            let state = backend.get_view_state().clone();
            if remote_player_count > 0 {
                send_reset(&mut ws_sender, &state);
            }
            event_tx
                .unbounded_send(SessionEvent::Update(ViewStateUpdate::Full(state)))
                .ok();
        } else {
            // Broadcast deltas, then notify local UI.
            if remote_player_count > 0 && !delta_batch.is_empty() {
                send_delta(&mut ws_sender, &delta_batch);
            }
            for d in delta_batch {
                event_tx
                    .unbounded_send(SessionEvent::Update(ViewStateUpdate::Incremental(d)))
                    .ok();
            }
        }

        // Send full state to clients that joined this iteration.
        if client_joined {
            send_full_state(&mut ws_sender, backend.get_view_state());
        }

        sleep_ms(2).await;
    }
}
