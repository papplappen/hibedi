//! The game's only contact with multiplayer: one seat in a shared game.
//!
//! Everything about finding other players (rooms, invitations, waiting
//! until the seats fill) happens before the game starts and outside of
//! it: in the web page (web/main.js), or natively in
//! [`crate::relay::join`]. Once the game runs, a session only syncs game
//! states between the seats:
//!
//! - wasm: [`crate::web::WebSession`], the page's peer-to-peer room,
//! - native: [`crate::relay::RelaySession`], a relay server.
//!
//! States are opaque JSON here; the game (de)serializes its own.

use serde_json::Value;

pub trait Session {
    /// Our seat; seat `n` plays the `n`-th side.
    fn seat(&self) -> usize;

    /// Processes incoming traffic and returns the latest game state the
    /// session settled on since the last call, if any: another player's,
    /// or one that won over ours. Adopting it is always correct.
    fn receive(&mut self) -> Option<Value>;

    /// Shares our new game state.
    fn publish(&mut self, state: Value);

    /// Whether every player is connected (as of the last `receive`).
    fn everyone_online(&self) -> bool;
}
