//! Wire protocol encoding/decoding helpers.
//!
//! Translates between raw WebSocket binary frames and typed Rust values using
//! postcard serialization and the message-type constants from the `protocol` crate.

use crate::traits::{SerializationCap, ViewStateUpdate};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use ewebsock::{WsMessage, WsSender};
use postcard::{from_bytes, take_from_bytes, to_stdvec};
use protocol::{
    CLIENT_DISCONNECTS, CLIENT_DISCONNECTS_SELF, CLIENT_GETS_KICKED, CLIENT_ID_SIZE, DELTA_UPDATE,
    FULL_UPDATE, HAND_SHAKE_RESPONSE, JoinRequest, NEW_CLIENT, RESET, SERVER_DISCONNECTS,
    SERVER_ERROR, SERVER_RPC,
};

// ---------------------------------------------------------------------------
// Inbound command types (relay → host)
// ---------------------------------------------------------------------------

pub enum ToServerCommand<A> {
    ClientJoin(u16),
    ClientLeft(u16),
    Rpc(u16, A),
    Error(String),
}

// ---------------------------------------------------------------------------
// Send helpers
// ---------------------------------------------------------------------------

fn send_binary(sender: &mut WsSender, data: &[u8]) {
    sender.send(WsMessage::Binary(data.to_vec()));
}

pub fn send_join_request(sender: &mut WsSender, req: &JoinRequest) -> Result<(), String> {
    let bytes = to_stdvec(req).map_err(|e| e.to_string())?;
    send_binary(sender, &bytes);
    Ok(())
}

pub fn send_rpc<A: SerializationCap>(sender: &mut WsSender, action: &A) {
    let raw = to_stdvec(action).expect("Failed to serialize RPC");
    let mut buf = BytesMut::with_capacity(1 + raw.len());
    buf.put_u8(SERVER_RPC);
    buf.put_slice(&raw);
    send_binary(sender, &buf);
}

pub fn send_delta<D: SerializationCap>(sender: &mut WsSender, deltas: &[D]) {
    let serialized: Vec<u8> = deltas
        .iter()
        .flat_map(|d| to_stdvec(d).expect("Failed to serialize delta"))
        .collect();
    let mut buf = BytesMut::with_capacity(1 + serialized.len());
    buf.put_u8(DELTA_UPDATE);
    buf.put_slice(&serialized);
    send_binary(sender, &buf);
}

pub fn send_full_state<VS: SerializationCap>(sender: &mut WsSender, state: &VS) {
    let serialized = to_stdvec(state).expect("Failed to serialize full state");
    let mut buf = BytesMut::with_capacity(1 + serialized.len());
    buf.put_u8(FULL_UPDATE);
    buf.put_slice(&serialized);
    send_binary(sender, &buf);
}

pub fn send_reset<VS: SerializationCap>(sender: &mut WsSender, state: &VS) {
    let serialized = to_stdvec(state).expect("Failed to serialize reset state");
    let mut buf = BytesMut::with_capacity(1 + serialized.len());
    buf.put_u8(RESET);
    buf.put_slice(&serialized);
    send_binary(sender, &buf);
}

pub fn send_kick(sender: &mut WsSender, player_id: u16) {
    let mut buf = BytesMut::with_capacity(1 + CLIENT_ID_SIZE);
    buf.put_u8(CLIENT_GETS_KICKED);
    buf.put_u16(player_id);
    send_binary(sender, &buf);
}

pub fn send_disconnect(sender: &mut WsSender, as_host: bool) {
    let msg = if as_host {
        SERVER_DISCONNECTS
    } else {
        CLIENT_DISCONNECTS_SELF
    };
    send_binary(sender, &[msg]);
}

// ---------------------------------------------------------------------------
// Receive / parse helpers
// ---------------------------------------------------------------------------

/// Parses the relay's handshake response.
///
/// Returns `(player_id, rule_variation, reconnect_token)`.
pub fn parse_handshake_response(data: Vec<u8>) -> Result<(u16, u16, u64), String> {
    let mut bytes = Bytes::from(data);
    let msg = bytes.get_u8();
    match msg {
        SERVER_ERROR => Err(String::from_utf8_lossy(&bytes).to_string()),
        HAND_SHAKE_RESPONSE => {
            let player_id = bytes.get_u16();
            let rule_variation = bytes.get_u16();
            let token = bytes.get_u64();
            Ok((player_id, rule_variation, token))
        }
        other => Err(format!("Unexpected handshake message id: {other}")),
    }
}

pub fn parse_server_command<A: SerializationCap>(data: Vec<u8>) -> ToServerCommand<A> {
    let mut bytes = Bytes::from(data);
    let msg = bytes.get_u8();
    match msg {
        SERVER_ERROR => ToServerCommand::Error(String::from_utf8_lossy(&bytes).to_string()),
        NEW_CLIENT => ToServerCommand::ClientJoin(bytes.get_u16()),
        CLIENT_DISCONNECTS => ToServerCommand::ClientLeft(bytes.get_u16()),
        SERVER_RPC => {
            let client_id = bytes.get_u16();
            let payload: A =
                from_bytes(bytes.chunk()).expect("Failed to deserialize server RPC payload");
            ToServerCommand::Rpc(client_id, payload)
        }
        other => ToServerCommand::Error(format!("Unknown server message id: {other}")),
    }
}

pub fn parse_client_update<VS, D>(
    data: Vec<u8>,
) -> Result<Vec<ViewStateUpdate<VS, D>>, String>
where
    VS: SerializationCap,
    D: SerializationCap,
{
    let mut bytes = Bytes::from(data);
    let msg = bytes.get_u8();
    match msg {
        SERVER_ERROR => Err(String::from_utf8_lossy(&bytes).to_string()),
        DELTA_UPDATE => {
            let mut result = Vec::new();
            let mut remaining: &[u8] = &bytes;
            while !remaining.is_empty() {
                let (delta, rest): (D, &[u8]) =
                    take_from_bytes(remaining).map_err(|e| e.to_string())?;
                remaining = rest;
                result.push(ViewStateUpdate::Incremental(delta));
            }
            Ok(result)
        }
        FULL_UPDATE | RESET => {
            let state: VS = from_bytes(&bytes).map_err(|e| e.to_string())?;
            Ok(vec![ViewStateUpdate::Full(state)])
        }
        other => Err(format!("Unknown client message id: {other}")),
    }
}
