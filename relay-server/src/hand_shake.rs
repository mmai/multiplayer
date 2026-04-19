//! This module does the whole initialization and handshake thing.
//! The general protocol of connecting is :
//! WASM Client -> Websocket: postcard serialized join request.
//! Websocket -> WASM Client: u16 player id, u16 rule variation, u64 reconnect token.

use crate::db;
use crate::hand_shake::ClientServerSpecificData::{Client, Server};
use crate::hand_shake::DisconnectEndpointSpecification::{DisconnectClient, DisconnectServer};
use crate::lobby::{AppState, Room};
use axum::extract::ws::Message::Binary;
use axum::extract::ws::{Message, WebSocket};
use bytes::{BufMut, Bytes, BytesMut};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{sink::SinkExt, stream::StreamExt};
use postcard::from_bytes;
use protocol::{
    CHANNEL_BUFFER_SIZE, CLIENT_DISCONNECT_MSG_SIZE, CLIENT_DISCONNECTS, HAND_SHAKE_RESPONSE,
    HAND_SHAKE_RESPONSE_SIZE, JoinRequest, NEW_CLIENT, NEW_CLIENT_MSG_SIZE,
    SERVER_DISCONNECT_MSG_SIZE, SERVER_DISCONNECTS, SERVER_ERROR,
};
use rand::random;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{broadcast, mpsc};

/// Is called on error, sends a text message because e-websocket can not interpret closing messages.
/// This text message is encoded as a binary message.
async fn send_closing_message(sender: &mut SplitSink<WebSocket, Message>, closing_message: String) {
    let raw_data = closing_message.as_bytes();
    let mut msg = BytesMut::with_capacity(1 + raw_data.len());
    msg.put_u8(SERVER_ERROR);
    msg.put_slice(raw_data);

    let _ = sender.send(Message::Binary(msg.into())).await;
    let _ = sender.send(Message::Close(None)).await;
}

/// The handshake result we get for the joining the room.
pub struct HandshakeResult {
    /// The id of the player we play.
    pub player_id: u16,
    /// The complete identifier of the room as stored in the hashmap.
    pub room_id: String,
    /// The rule variation we apply.
    pub rule_variation: u16,
    /// The reconnect token for this player — sent back to the client for localStorage storage.
    pub token: u64,
    /// The internal connection information.
    pub specific_data: ClientServerSpecificData,
}

/// Contains all the channel information for internal communication.
pub enum ClientServerSpecificData {
    /// In this case we are servicing the server.
    Server(Receiver<Bytes>, broadcast::Sender<Bytes>),
    /// In this case we are servicing a client.
    Client(broadcast::Receiver<Bytes>, Sender<Bytes>),
}

/// This data is data we need to keep for the disconnect handling and cleanup.
pub struct DisconnectData {
    /// The id of the player we play.
    pub player_id: u16,
    /// The complete identifier of the room as stored in the hashmap.
    pub room_id: String,
    /// The sender we use.
    pub sender: DisconnectEndpointSpecification,
}

/// Contains the information where to send error data to in case of disconnection.
pub enum DisconnectEndpointSpecification {
    /// If we are servicing the server, we broadcast the info to all clients.
    DisconnectServer(broadcast::Sender<Bytes>),
    /// If we are servicing the client, we send data to the server.
    DisconnectClient(Sender<Bytes>),
}

/// Construction of DisconnectData from Handshake result.
impl From<&HandshakeResult> for DisconnectData {
    fn from(value: &HandshakeResult) -> Self {
        match &value.specific_data {
            Server(_, internal_sender) => DisconnectData {
                player_id: value.player_id,
                room_id: value.room_id.clone(),
                sender: DisconnectServer(internal_sender.clone()),
            },
            Client(_, internal_sender) => DisconnectData {
                player_id: value.player_id,
                room_id: value.room_id.clone(),
                sender: DisconnectClient(internal_sender.clone()),
            },
        }
    }
}

/// Gets an initial connection result, where a room is constructed
/// and game and existence / non existence of room is checked for legality.
struct InitialConnectionResult {
    /// Flags, if we are a server.
    is_server: bool,
    /// The complete room we have for internal administration.
    compound_room_id: String,
    /// Which game do we want to join.
    game_id: String,
    /// Which room do we want to join.
    room_id: String,
    /// The rule variation that is applied, this gets only interpreted if a room gets constructed.
    rule_variation: u16,
    /// The maximum amount of players a room allows (0 = infinite).
    max_players: u16,
    /// Reconnect token from the client, if this is a reconnect attempt.
    reconnect_token: Option<u64>,
}

/// Reads in the join request from the web socket, verifies if game exists and generates the final room name.
async fn get_initial_query(
    sender: &mut SplitSink<WebSocket, Message>,
    receiver: &mut SplitStream<WebSocket>,
    state: Arc<AppState>,
) -> Option<InitialConnectionResult> {
    // First we get a room opening and joining request. This is the first binary message we received.
    let my_data = loop {
        let Some(raw_data) = receiver.next().await else {
            tracing::warn!("WebSocket closed before handshake completed");
            send_closing_message(sender, "Initial error during handshake.".into()).await;
            return None;
        };
        match raw_data {
            Err(err) => {
                tracing::error!(?err, "Initial error during handshake.");
                send_closing_message(sender, "Initial error during handshake.".into()).await;
                return None;
            }
            Ok(Binary(data)) => {
                break data;
            }
            // We do not care about any other message like ping pong messages.
            Ok(_) => {}
        }
    };

    // Now we get some data and we try to convert it into the required format.
    let working_struct = match from_bytes::<JoinRequest>(&my_data) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to parse join request");
            send_closing_message(sender, "Failed to parse join request.".into()).await;
            return None;
        }
    };

    // Let us take a look, if the game exists.
    let games = state.configs.read().await;
    let game_exists = games.contains_key(&working_struct.game_id);
    let max_players = if game_exists {
        games[&working_struct.game_id]
    } else {
        0
    };
    drop(games);

    if !game_exists {
        tracing::error!(
            optional_game = working_struct.game_id,
            "Requested illegal game."
        );
        send_closing_message(sender, format!("Unknown game {}.", &working_struct.game_id)).await;
        return None;
    }

    // The final room id is the combination of game and room id.
    let room_id = format!(
        "{}#{}",
        working_struct.room_id.as_str(),
        working_struct.game_id.as_str()
    );
    let is_server = working_struct.create_room;

    Some(InitialConnectionResult {
        is_server,
        compound_room_id: room_id,
        game_id: working_struct.game_id,
        room_id: working_struct.room_id,
        rule_variation: working_struct.rule_variation,
        max_players,
        reconnect_token: working_struct.reconnect_token,
    })
}

/// Connects and eventually establishes a room.
pub async fn init_and_connect(
    sender: &mut SplitSink<WebSocket, Message>,
    receiver: &mut SplitStream<WebSocket>,
    state: Arc<AppState>,
    user_id: Option<i64>,
) -> Option<HandshakeResult> {
    let start_result = get_initial_query(sender, receiver, state.clone()).await?;

    if let Some(token) = start_result.reconnect_token {
        process_handshake_reconnect(sender, state, start_result, token, user_id).await
    } else if start_result.is_server {
        process_handshake_server(sender, state, start_result, user_id).await
    } else {
        process_handshake_client(sender, state, start_result, user_id).await
    }
}

/// Does the handshake, if we are connected to a client.
async fn process_handshake_client(
    sender: &mut SplitSink<WebSocket, Message>,
    state: Arc<AppState>,
    initial_result: InitialConnectionResult,
    user_id: Option<i64>,
) -> Option<HandshakeResult> {
    let mut rooms = state.rooms.lock().await;
    let Some(local_room) = rooms.get_mut(&initial_result.compound_room_id) else {
        drop(rooms);
        send_closing_message(
            sender,
            format!(
                "Room {} does not exist for game {}.",
                &initial_result.room_id, &initial_result.game_id
            ),
        )
        .await;
        return None;
    };

    // Do we fit in? max_players == 0 means "infinite".
    if initial_result.max_players != 0 && local_room.amount_of_players >= initial_result.max_players
    {
        drop(rooms);
        send_closing_message(
            sender,
            format!(
                "Room  {} exceeded max amount of players {}.",
                &initial_result.room_id, initial_result.max_players
            ),
        )
        .await;
        return None;
    }

    // Save guard against the case, that we have run out of client ids.
    if local_room.next_client_id > u16::MAX - 100 {
        drop(rooms);
        send_closing_message(
            sender,
            format!("Room {} run out of client ids.", &initial_result.room_id),
        )
        .await;
        tracing::error!("Server run out of client ids.");
        return None;
    }

    local_room.amount_of_players += 1;
    let player_id = local_room.next_client_id;
    local_room.next_client_id += 1;

    let token: u64 = random();
    local_room.player_tokens.insert(player_id, token);
    local_room.connected_players.push(player_id);
    local_room.user_ids.insert(player_id, user_id);

    let to_server_sender = local_room.to_host_sender.clone();
    let receiver = local_room.host_to_client_broadcaster.subscribe();
    let rule_variation = local_room.rule_variation;
    drop(rooms);

    // Here we send a message to the server, that a new client has joined.
    let mut msg = BytesMut::with_capacity(NEW_CLIENT_MSG_SIZE);
    msg.put_u8(NEW_CLIENT); // Message-Type
    msg.put_u16(player_id); // player id.

    let result = to_server_sender.send(msg.into()).await;
    if let Err(error) = result {
        // We have to leave the room again.
        let mut rooms = state.rooms.lock().await;
        if let Some(room) = rooms.get_mut(&initial_result.compound_room_id) {
            room.amount_of_players -= 1;
            room.player_tokens.remove(&player_id);
        }
        drop(rooms);
        tracing::error!(?error, "Server unexpectedly left during handshake");
        send_closing_message(sender, "Server unexpectedly left during handshake".into()).await;
        return None;
    }

    Some(HandshakeResult {
        room_id: initial_result.compound_room_id,
        player_id,
        rule_variation,
        token,
        specific_data: Client(receiver, to_server_sender),
    })
}

/// Opens a new room and generates the handshake result for the server.
async fn process_handshake_server(
    sender: &mut SplitSink<WebSocket, Message>,
    state: Arc<AppState>,
    initial_result: InitialConnectionResult,
    user_id: Option<i64>,
) -> Option<HandshakeResult> {
    // Insert a game record before taking the rooms lock (best-effort: failures don't abort the handshake).
    let game_record_id =
        match db::insert_game_record(&state.db, &initial_result.game_id, &initial_result.room_id)
            .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!("Failed to create game record for room {}: {e}", initial_result.room_id);
                None
            }
        };

    let mut rooms = state.rooms.lock().await;
    if rooms.contains_key(&initial_result.compound_room_id) {
        drop(rooms);
        send_closing_message(
            sender,
            format!(
                "Room {} already exists for game {}.",
                &initial_result.room_id, &initial_result.game_id
            ),
        )
        .await;
        // User error no need for error tracing.
        return None;
    }
    // Here we create a new room.
    let (to_server_sender, to_server_receiver) = mpsc::channel(CHANNEL_BUFFER_SIZE);
    let (to_client_sender, _) = broadcast::channel(CHANNEL_BUFFER_SIZE);
    let token: u64 = random();
    let mut player_tokens = HashMap::new();
    player_tokens.insert(0u16, token);
    let mut user_ids = HashMap::new();
    user_ids.insert(0u16, user_id);
    let new_room = Room {
        next_client_id: 1,
        amount_of_players: 1,
        rule_variation: initial_result.rule_variation,
        to_host_sender: to_server_sender,
        host_to_client_broadcaster: to_client_sender.clone(),
        player_tokens,
        host_connected: true,
        connected_players: Vec::new(),
        game_record_id,
        user_ids,
    };
    rooms.insert(initial_result.compound_room_id.clone(), new_room);
    drop(rooms);
    let hand_shake_result = HandshakeResult {
        room_id: initial_result.compound_room_id,
        player_id: 0,
        rule_variation: initial_result.rule_variation,
        token,
        specific_data: Server(to_server_receiver, to_client_sender),
    };
    Some(hand_shake_result)
}

/// Reconnects a previously connected player (host or client) using their stored token.
///
/// **Client reconnect**: resubscribes to the broadcast channel and notifies the host
/// via `NEW_CLIENT` so it delivers a fresh `FULL_UPDATE`.
///
/// **Host reconnect**: creates a new mpsc channel (the old one died with the WebSocket),
/// replaces `room.to_host_sender`, and queues `NEW_CLIENT` / `CLIENT_DISCONNECTS`
/// messages so the host backend can reconstruct who is currently in the room.
async fn process_handshake_reconnect(
    sender: &mut SplitSink<WebSocket, Message>,
    state: Arc<AppState>,
    initial_result: InitialConnectionResult,
    reconnect_token: u64,
    user_id: Option<i64>,
) -> Option<HandshakeResult> {
    let mut rooms = state.rooms.lock().await;
    let Some(local_room) = rooms.get_mut(&initial_result.compound_room_id) else {
        drop(rooms);
        send_closing_message(
            sender,
            format!(
                "Room {} no longer exists for game {}.",
                &initial_result.room_id, &initial_result.game_id
            ),
        )
        .await;
        return None;
    };

    // Find the player whose token matches.
    let player_id = match local_room
        .player_tokens
        .iter()
        .find(|&(_, &t)| t == reconnect_token)
        .map(|(&id, _)| id)
    {
        Some(id) => id,
        None => {
            drop(rooms);
            tracing::warn!("Reconnect attempt with invalid token in room {}", &initial_result.room_id);
            send_closing_message(sender, "Invalid reconnect token.".into()).await;
            return None;
        }
    };

    // ------------------------------------------------------------------ Host reconnect
    if player_id == 0 {
        if local_room.host_connected {
            drop(rooms);
            send_closing_message(sender, "Host is already connected.".into()).await;
            return None;
        }

        // Create a fresh mpsc channel (the previous receiver was dropped when the
        // host's WebSocket closed).
        let (new_sender, new_receiver) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        local_room.to_host_sender = new_sender.clone();
        local_room.host_connected = true;
        local_room.user_ids.insert(0u16, user_id);

        let broadcaster = local_room.host_to_client_broadcaster.clone();
        let rule_variation = local_room.rule_variation;

        // Collect the players we need to notify about.
        let connected = local_room.connected_players.clone();
        let all_non_host: Vec<u16> = local_room
            .player_tokens
            .keys()
            .filter(|&&pid| pid != 0)
            .copied()
            .collect();
        drop(rooms);

        // Queue NEW_CLIENT for every currently connected player so the host backend
        // increments remote_player_count and sends a FULL_UPDATE.
        for pid in &connected {
            let mut msg = BytesMut::with_capacity(NEW_CLIENT_MSG_SIZE);
            msg.put_u8(NEW_CLIENT);
            msg.put_u16(*pid);
            let _ = new_sender.send(msg.into()).await;
        }
        // Queue CLIENT_DISCONNECTS for players who left while the host was away so
        // the backend can start their grace-period timers.
        for pid in all_non_host {
            if !connected.contains(&pid) {
                let mut msg = BytesMut::with_capacity(CLIENT_DISCONNECT_MSG_SIZE);
                msg.put_u8(CLIENT_DISCONNECTS);
                msg.put_u16(pid);
                let _ = new_sender.send(msg.into()).await;
            }
        }

        tracing::info!(room = &initial_result.room_id, "Host reconnected");

        return Some(HandshakeResult {
            room_id: initial_result.compound_room_id,
            player_id: 0,
            rule_variation,
            token: reconnect_token,
            specific_data: Server(new_receiver, broadcaster),
        });
    }

    // ---------------------------------------------------------------- Client reconnect
    local_room.amount_of_players += 1;
    local_room.connected_players.push(player_id);
    local_room.user_ids.insert(player_id, user_id);
    let to_server_sender = local_room.to_host_sender.clone();
    let broadcast_receiver = local_room.host_to_client_broadcaster.subscribe();
    let rule_variation = local_room.rule_variation;
    drop(rooms);

    // Notify the host that this player has rejoined so it sends a FULL_UPDATE.
    let mut msg = BytesMut::with_capacity(NEW_CLIENT_MSG_SIZE);
    msg.put_u8(NEW_CLIENT);
    msg.put_u16(player_id);

    if let Err(error) = to_server_sender.send(msg.into()).await {
        let mut rooms = state.rooms.lock().await;
        if let Some(room) = rooms.get_mut(&initial_result.compound_room_id) {
            room.amount_of_players -= 1;
            room.connected_players.retain(|&p| p != player_id);
        }
        drop(rooms);
        tracing::error!(?error, "Host unavailable during reconnect handshake");
        send_closing_message(sender, "Host is no longer available.".into()).await;
        return None;
    }

    tracing::info!(
        player_id,
        room = &initial_result.room_id,
        "Player reconnected"
    );

    Some(HandshakeResult {
        room_id: initial_result.compound_room_id,
        player_id,
        rule_variation,
        token: reconnect_token,
        specific_data: Client(broadcast_receiver, to_server_sender),
    })
}

/// Informs the partner of the connection result, returns a bool as a success flag.
pub async fn inform_client_of_connection(
    sender: &mut SplitSink<WebSocket, Message>,
    status: &HandshakeResult,
) -> bool {
    let mut msg = BytesMut::with_capacity(HAND_SHAKE_RESPONSE_SIZE);
    msg.put_u8(HAND_SHAKE_RESPONSE);
    msg.put_u16(status.player_id);
    msg.put_u16(status.rule_variation);
    msg.put_u64(status.token);

    let result = sender.send(Message::Binary(msg.into())).await;
    result.is_ok()
}

/// Performs the shutdown of the system and sends a last message.
pub async fn shutdown_connection(
    wrapped_sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    disconnect_data: DisconnectData,
    app_state: Arc<AppState>,
    error_message: &'static str,
) {
    match disconnect_data.sender {
        DisconnectServer(broadcaster) => {
            // Mark the host as disconnected and start a 30-second grace period.
            // If the host reconnects within that window the grace task does nothing;
            // otherwise it broadcasts SERVER_DISCONNECTS and removes the room.
            {
                let mut rooms = app_state.rooms.lock().await;
                if let Some(room) = rooms.get_mut(&disconnect_data.room_id) {
                    room.host_connected = false;
                }
            }

            let state_clone = app_state.clone();
            let room_id = disconnect_data.room_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

                let game_record_id = {
                    let mut rooms = state_clone.rooms.lock().await;
                    if let Some(room) = rooms.get(&room_id) {
                        if !room.host_connected {
                            let record_id = room.game_record_id;
                            rooms.remove(&room_id);
                            record_id
                        } else {
                            return; // host reconnected
                        }
                    } else {
                        return; // room already removed
                    }
                };

                // Room lock released — broadcast and close the DB record.
                let mut msg = BytesMut::with_capacity(SERVER_DISCONNECT_MSG_SIZE);
                msg.put_u8(SERVER_DISCONNECTS);
                let _ = broadcaster.send(msg.into());
                tracing::info!(room_id, "Host grace period expired — room removed");

                if let Some(record_id) = game_record_id {
                    if let Err(e) = db::close_game_record(&state_clone.db, record_id, None).await {
                        tracing::warn!("Failed to close game record {record_id}: {e}");
                    }
                }
            });
        }
        DisconnectClient(sender) => {
            // Inform server first.
            let mut msg = BytesMut::with_capacity(CLIENT_DISCONNECT_MSG_SIZE);
            msg.put_u8(CLIENT_DISCONNECTS);
            msg.put_u16(disconnect_data.player_id);
            let _ = sender.send(msg.into()).await;
            // Subtract one client from the room.
            let mut rooms = app_state.rooms.lock().await;
            // Check if the room still exists.
            if let Some(room) = rooms.get_mut(&disconnect_data.room_id) {
                room.amount_of_players -= 1;
                room.connected_players.retain(|&p| p != disconnect_data.player_id);
                // Note: we intentionally keep the token in player_tokens so the
                // client can use it to reconnect as long as the room exists.
            }
            drop(rooms);
        }
    }

    let mut sender = wrapped_sender.lock().await;

    // Send the message to the WASM point.
    send_closing_message(&mut sender, error_message.into()).await;
}
