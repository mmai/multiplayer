use serde::Serialize;
use serde::de::DeserializeOwned;

/// Marker trait for types that can be serialized with postcard.
pub trait SerializationCap: Serialize + DeserializeOwned {}
impl<T> SerializationCap for T where T: Serialize + DeserializeOwned {}

/// State updates delivered to the frontend for rendering.
///
/// - [`Full`](Self::Full): Immediately set all visual state, no animation.
/// - [`Incremental`](Self::Incremental): Apply with animation/transition.
pub enum ViewStateUpdate<ViewState, DeltaInformation> {
    /// Complete game state snapshot. Received on join or after a reset.
    Full(ViewState),
    /// Incremental state change for animated transitions.
    Incremental(DeltaInformation),
}

/// Commands emitted by the game backend to control the session.
pub enum BackendCommand<DeltaInformation>
where
    DeltaInformation: SerializationCap,
{
    /// Incremental state change to be broadcast to all frontends.
    Delta(DeltaInformation),

    /// Signals a complete reset: discard queued deltas, broadcast fresh full state.
    ResetViewState,

    /// Forcibly removes a player from the session.
    KickPlayer { player: u16 },

    /// Schedules a callback after `duration` seconds. Overwrites any existing
    /// timer with the same `timer_id`.
    SetTimer { timer_id: u16, duration: f32 },

    /// Cancels a previously scheduled timer. No-op if already fired or not set.
    CancelTimer { timer_id: u16 },

    /// Shuts down the entire room and disconnects all players.
    TerminateRoom,
}

/// The contract for game-specific server logic.
///
/// Implement this on the host side. The session calls these methods in response
/// to network events and drives `drain_commands` to collect outbound messages.
///
/// # Type Parameters
/// * `ServerRpcPayload` — Actions sent by players (e.g. `PlacePiece { x, y }`)
/// * `DeltaInformation` — Incremental state changes for animations
/// * `ViewState` — Complete game snapshot for syncing new clients
pub trait BackEndArchitecture<ServerRpcPayload, DeltaInformation, ViewState>
where
    ServerRpcPayload: SerializationCap,
    DeltaInformation: SerializationCap,
    ViewState: SerializationCap + Clone,
{
    /// Creates a new game instance. `rule_variation` selects the game mode.
    fn new(rule_variation: u16) -> Self;

    /// Attempt to restore a previously running game from serialized bytes.
    ///
    /// Called when the host reconnects after a page refresh. The bytes are the
    /// game-specific snapshot produced by the app layer (via `serde_json` or
    /// similar) and stored in localStorage.
    ///
    /// Return `None` if restoration is not supported or the bytes are invalid —
    /// the caller falls back to `new(rule_variation)`.
    fn from_bytes(_rule_variation: u16, _bytes: &[u8]) -> Option<Self>
    where
        Self: Sized,
    {
        None
    }

    /// Called when a player connects. Player will receive a full state snapshot
    /// automatically after this returns.
    fn player_arrival(&mut self, player: u16);

    /// Called when a player disconnects.
    fn player_departure(&mut self, player: u16);

    /// Called when a player sends a game action.
    fn inform_rpc(&mut self, player: u16, payload: ServerRpcPayload);

    /// Called when a previously scheduled timer fires.
    fn timer_triggered(&mut self, timer_id: u16);

    /// Returns the complete current game state.
    fn get_view_state(&self) -> &ViewState;

    /// Collects and clears all pending commands since the last drain.
    ///
    /// Implement with `std::mem::take(&mut self.command_list)`.
    fn drain_commands(&mut self) -> Vec<BackendCommand<DeltaInformation>>;
}
