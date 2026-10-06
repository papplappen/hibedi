//! The web page around the wasm game (web/game.js). The page runs the
//! rooms; the game only asks it for our seat, game states other players
//! published and whether everyone is connected, and hands it the game
//! states we publish. States cross as JSON text.
//!
//! The page keeps what the room sends until we ask, so a game already
//! in progress is there when the game starts.

use bevy::log::warn;
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::session::Session;

#[wasm_bindgen]
extern "C" {
    /// Our seat in the page's room; `null` for a local game.
    #[wasm_bindgen(js_namespace = hibedi, js_name = seat)]
    fn page_seat() -> Option<u32>;

    /// The latest game state another player published, if one arrived
    /// since the last call.
    #[wasm_bindgen(js_namespace = hibedi, js_name = takeState)]
    fn page_take_state() -> Option<String>;

    /// Whether every seat's player is connected.
    #[wasm_bindgen(js_namespace = hibedi, js_name = online)]
    fn page_online() -> bool;

    /// Hands the page a game state we publish.
    #[wasm_bindgen(js_namespace = hibedi, js_name = publish)]
    fn page_publish(state: &str);
}

/// Our seat in the page's room; `None` when the page runs a local game.
pub fn session() -> Option<WebSession> {
    page_seat().map(|seat| WebSession {
        seat: seat as usize,
    })
}

pub struct WebSession {
    seat: usize,
}

impl Session for WebSession {
    fn seat(&self) -> usize {
        self.seat
    }

    fn receive(&mut self) -> Option<Value> {
        let state = page_take_state()?;
        serde_json::from_str(&state)
            .inspect_err(|error| warn!("ignoring a malformed game state from the page: {error}"))
            .ok()
    }

    fn publish(&mut self, state: Value) {
        page_publish(&state.to_string());
    }

    fn everyone_online(&self) -> bool {
        page_online()
    }
}
