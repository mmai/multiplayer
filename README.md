# Introduction

This is a Leptos-optimized adaptation of the macroquad-based [Carbonfreezer/multiplayer](https://github.com/Carbonfreezer/multiplayer) project. It is a multiplayer game system in Rust targeting browser-based board games compiled as WASM. The original project used Macroquad with a polling-based transport layer; this fork replaces that with an async session API built for [Leptos](https://leptos.dev/).

The system consists of:

- A **relay server** (Axum/Tokio) that routes messages between players and manages rooms, without knowing anything about game rules.
- A **backbone library** that handles WebSocket connection, handshake, and message routing, exposing an async API to the game frontend.
- Game-specific **backend logic** implementing the `BackEndArchitecture` trait, which runs only on the hosting client.
- A **Dioxus frontend** that connects to a session and reacts to state updates.

# Architecture

## Client-hosted server model

There is no dedicated game server. One of the players acts as the host: their browser runs the game backend locally. The relay server only forwards messages — it never touches game state.

```
┌─────────────────────────────────────────────────────────────┐
│                        Host Client                          │
│  ┌─────────────┐    ┌──────────────────┐    ┌────────────┐  │
│  │  Dioxus UI  │◄──►│  GameSession     │◄──►│  Backend   │  │
│  └─────────────┘    └────────┬─────────┘    └────────────┘  │
└───────────────────────────── │ ────────────────────────────┘
                                │  WebSocket
                         ┌──────▼──────┐
                         │ Relay Server│
                         └──────┬──────┘
                                │  WebSocket
┌───────────────────────────────│────────────────────────────┐
│  ┌─────────────┐    ┌─────────▼────────┐                   │
│  │  Dioxus UI  │◄──►│  GameSession     │  (no backend)     │
│  └─────────────┘    └──────────────────┘                   │
│                        Remote Client                        │
└─────────────────────────────────────────────────────────────┘
```

## Data flow

- **Actions** (e.g. "place stone at B3") flow from the UI to the host backend via `GameSession::send_action()`.
- **State updates** flow back as `ViewStateUpdate::Full` (full snapshot, on join or reset) or `ViewStateUpdate::Incremental` (delta, for animations).
- **Timers** are managed by the host's background task (wall-clock, no polling required from the game).

## backbone-lib session API

The key design choice: `backbone-lib` owns a background async task per session. The Leptos app never drives a loop — it just awaits on events.

# Workspace

## Protocol

Shared message-type constants and the `JoinRequest` struct used during the WebSocket handshake.

## Relay Server

Listens on port 8080. Loads `GameConfig.json` on startup to know which games exist and their player limits:

```json
[{ "name": "tic-tac-toe", "max_players": 10 }]
```

Games can be added at runtime via the `/reload` endpoint. `/enlist` lists active rooms. A watchdog cleans up inactive rooms every 20 minutes.

For production, put it behind a reverse proxy with SSL (the browser requires `wss://` on HTTPS pages). Example Caddy config:

```
your-domain.com {
    handle_path /api/* {
        reverse_proxy localhost:8080
    }
    file_server
}
```

## Backbone Library

Modules:

| Module     | Purpose                                                                                                    |
| ---------- | ---------------------------------------------------------------------------------------------------------- |
| `session`  | `GameSession`, `connect()`, `SessionEvent`, `RoomConfig`                                                   |
| `host`     | Background async task for the hosting client (drives `BackEndArchitecture`, manages timers)                |
| `client`   | Background async task for non-hosting clients                                                              |
| `protocol` | Wire encoding/decoding helpers (postcard + message-type bytes)                                             |
| `platform` | `spawn_task` / `sleep_ms` abstractions (WASM: `spawn_local` + gloo-timers; native: thread + thread::sleep) |
| `traits`   | `BackEndArchitecture`, `BackendCommand`, `ViewStateUpdate`, `SerializationCap`                             |

## Implementing BackEndArchitecture

The only thing a game needs to implement on the host side:

```rust
impl BackEndArchitecture<MyAction, MyDelta, MyState> for MyBackend {
    fn new(rule_variation: u16) -> Self { /* construct */ }
    fn player_arrival(&mut self, player: u16) { /* ... */ }
    fn player_departure(&mut self, player: u16) { /* ... */ }
    fn inform_rpc(&mut self, player: u16, action: MyAction) { /* update state, push BackendCommand */ }
    fn timer_triggered(&mut self, timer_id: u16) { /* ... */ }
    fn get_view_state(&self) -> &MyState { &self.state }
    fn drain_commands(&mut self) -> Vec<BackendCommand<MyDelta>> {
        std::mem::take(&mut self.commands)
    }
}
```

`drain_commands` returns a `Vec<BackendCommand<MyDelta>>` which can contain:

- `Delta(d)` — broadcast incremental update to all clients
- `ResetViewState` — broadcast full state (e.g. new round)
- `SetTimer { timer_id, duration }` / `CancelTimer { timer_id }` — wall-clock callbacks
- `KickPlayer { player }` — forcibly disconnect a player
- `TerminateRoom` — shut down the session

## Leptos Tic-Tac-Toe

A minimal working example in `games/leptos-tic-tac-toe`. Shows:

- Connecting as host or client from a login screen
- Rendering the board from `ViewStateUpdate` events
- Sending `StonePlacement` RPCs on cell click
- Handling disconnection back to the login screen

# Getting started

Install [devenv](https://devenv.sh/getting-started/), start a devenv shell `devenv shell`, and run the following commands.

```bash
# Run the relay server
just build-relay
just run-relay  # listens on :8080

# Run the Leptos game (separate terminal)
just dev-leptos
```

Open two browser windows at `http://127.0.0.1:9091`. In one, create a room; in the other, join with the same room name.

# Known limitations

- **Single WebSocket per session**: by design.
- **Host leaves = game over**: `TerminateRoom` is emitted by the backend when the host's player departs.

# License

MIT — see [LICENSE](LICENSE).
