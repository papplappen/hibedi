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

use hibedi_core::{ClientMessage, ServerMessage};

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
// Each target never constructs the other's variants: native never emits
// Closed/Booted/Room/Copied, wasm never emits Disconnected.
#[allow(dead_code)]
pub enum NetEvent {
    Connected,
    #[cfg(not(target_arch = "wasm32"))]
    Disconnected,
    /// Terminal disconnect (wasm P2P only): a left peer cannot be
    /// reconnected to automatically, so the session is over.
    Closed { reason: String },
    Message(ServerMessage),
    /// The bridge is up; `room` is a room code carried by the page URL.
    Booted { room: Option<String> },
    /// A hosted room exists and is waiting for the opponent.
    Room { code: String, share_url: String },
    /// The bridge copied text to the clipboard.
    Copied,
}

/// JSON envelope for commands from Rust into the JS bridge (web/net.js).
/// The full wire format is declared on every target even though some
/// variants are only ever constructed on one of them.
#[derive(Debug, Serialize)]
#[allow(dead_code, clippy::large_enum_variant)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum BridgeCommand {
    Host,
    Join { code: String },
    Client { message: ClientMessage },
    Server {
        message: ServerMessage,
        /// Target peer for directed sends; `None` broadcasts to the room.
        peer: Option<String>,
    },
    Copy { text: String },
    Leave,
}

/// JSON envelope for events from the JS bridge into Rust. Same deal as
/// [`BridgeCommand`]: declared everywhere, driven only on wasm.
#[derive(Debug, Deserialize)]
#[allow(dead_code, clippy::large_enum_variant)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum BridgeEvent {
    Booted { room: Option<String> },
    Room { code: String, share_url: String },
    Joined,
    PeerJoined { peer: String },
    PeerLeft { peer: String },
    Client { peer: String, message: ClientMessage },
    Server { message: ServerMessage },
    Copied,
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{NetHandle, spawn};

#[cfg(target_arch = "wasm32")]
pub use wasm::{NetHandle, bridge_ready};
