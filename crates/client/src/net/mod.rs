//! Client networking: a WebSocket server connection on native targets and
//! a Trystero/WebRTC bridge on wasm. Both expose the same [`NetHandle`]
//! interface and the same [`NetEvent`] stream, so game code is
//! transport-agnostic.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

// Hosted-room glue: only wired up on wasm, but compiled everywhere so its
// unit tests run on native targets.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod host;

use serde::{Deserialize, Serialize};

use hibedi_core::ServerMessage;

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
// Each target never constructs the other's variants: native never emits
// Closed/Lobby/LocalGame, wasm never emits Disconnected.
#[allow(dead_code)]
pub enum NetEvent {
    Connected,
    #[cfg(not(target_arch = "wasm32"))]
    Disconnected,
    /// Terminal disconnect (wasm P2P only): a left peer cannot be
    /// reconnected to automatically, so the session is over.
    Closed { reason: String },
    Message(ServerMessage),
    /// The bridge returned to the lobby (wasm): drop any session state.
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
    /// Hands `data` to the room; `peer` targets one peer, `None`
    /// broadcasts. Routing (guest→host, host→guest) is the bridge's job.
    Send {
        data: serde_json::Value,
        peer: Option<String>,
    },
}

/// How the lobby set us up. Reported with [`BridgeEvent::Start`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StartRole {
    /// The lobby opened a room; this client runs the authoritative session.
    Host,
    /// The lobby joined a room; the host runs the session.
    Guest,
    /// No room: a local hotseat game.
    Local,
}

/// JSON envelope for events from the JS bridge into Rust. The bridge owns
/// the lobby (host / join / local choice and the room itself); these
/// events hand over the connection and report its traffic. `data` is an
/// opaque JSON value; its interpretation depends on the role. Declared on
/// every target, driven only on wasm.
#[derive(Debug, Deserialize)]
#[allow(dead_code, clippy::large_enum_variant)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum BridgeEvent {
    /// The lobby decided how to run; the connection is live.
    Start { role: StartRole },
    /// The lobby returned to its menu; drop any room state.
    Leave,
    PeerJoined { peer: String },
    PeerLeft { peer: String },
    /// A message from the room: guest→host traffic carries client
    /// messages, host→guest traffic carries server messages.
    Message { peer: String, data: serde_json::Value },
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{NetHandle, spawn};

#[cfg(target_arch = "wasm32")]
pub use wasm::{NetHandle, bridge_ready};
