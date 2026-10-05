//! Client networking: a WebSocket relay connection on native targets and
//! a Trystero/WebRTC bridge on wasm. Both expose the same [`NetHandle`]
//! interface and the same [`NetEvent`] stream, so game code is
//! transport-agnostic. Replication is symmetric: every seat plays its
//! local game and exchanges [`StateSync`] announcements; no seat is
//! authoritative.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

use serde::{Deserialize, Serialize};

use hibedi_core::{Side, StateSync};

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
// Each target never constructs the other's variants: native never emits
// Closed's lobby variants, wasm never emits Disconnected.
#[allow(dead_code)]
pub enum NetEvent {
    /// The connection is live and our seat is assigned.
    Connected { side: Side },
    /// Native: the relay connection dropped; we are reconnecting.
    #[cfg(not(target_arch = "wasm32"))]
    Disconnected,
    /// Terminal disconnect (native: rejected by the server; wasm: a left
    /// peer cannot be reconnected to automatically, so the room is over).
    Closed { reason: String },
    /// The other seat filled (or refilled after a reconnect); `peer` is
    /// the WebRTC peer id on wasm, `None` on native.
    PeerJoined { peer: Option<String> },
    /// The other seat is empty; the game pauses until they return.
    PeerLeft { peer: Option<String> },
    /// A state announcement from the other player.
    Sync(StateSync),
    /// The bridge returned to the lobby (wasm): drop game state.
    #[cfg(target_arch = "wasm32")]
    Lobby,
    /// The bridge asked for a local hotseat game (wasm).
    #[cfg(target_arch = "wasm32")]
    LocalGame,
}

/// JSON envelope for commands from Rust into the JS bridge (web/game.js).
/// Game-agnostic: payloads are opaque JSON values the bridge only routes.
/// Declared on every target even though only wasm constructs them.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum BridgeCommand {
    /// The game view is live; the bridge hides its lobby overlay.
    Ready,
    /// The player left the game; the lobby returns to its menu.
    Leave,
    /// Hands `data` to the room; `peer` targets one peer, `None`
    /// broadcasts.
    Send {
        data: serde_json::Value,
        peer: Option<String>,
    },
}

/// How the lobby set us up. Reported with [`BridgeEvent::Start`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StartRole {
    /// The lobby opened a room; this client holds the red seat and
    /// generates the first game when the guest arrives.
    Host,
    /// The lobby joined a room; this client holds the green seat and
    /// adopts the host's state.
    Guest,
    /// No room: a local hotseat game.
    Local,
}

/// JSON envelope for events from the JS bridge into Rust. The bridge owns
/// the lobby (host / join / local choice and the room itself); these
/// events hand over the connection and report its traffic. `data` is an
/// opaque JSON value carrying a [`StateSync`]. Declared on every target,
/// driven only on wasm.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum BridgeEvent {
    /// The lobby decided how to run; the connection is live.
    Start { role: StartRole },
    /// The lobby returned to its menu; drop any room state.
    Leave,
    PeerJoined { peer: String },
    PeerLeft { peer: String },
    /// A state announcement from the other seat.
    Message { peer: String, data: serde_json::Value },
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{NetHandle, spawn};

#[cfg(target_arch = "wasm32")]
pub use wasm::{NetHandle, bridge_ready};
