//! The web page around the wasm game (web/game.js). The page runs the
//! rooms; the game only exchanges JSON messages with it:
//!
//! - in: our seat (once, before `main`), game states other players
//!   published, and whether every player is connected,
//! - out: the game states we publish.
//!
//! The page delivers a message by writing it into the buffer from
//! [`hibedi_alloc`] and calling [`hibedi_receive`].

use std::cell::RefCell;

use serde::Deserialize;
use serde_json::Value;

use crate::session::Session;

unsafe extern "C" {
    /// Hands the page a game state we publish (JSON bytes, valid only
    /// for the duration of the call).
    fn hibedi_send(ptr: *const u8, len: u32);
    /// Fills `ptr[0..len]` with random bytes (crypto.getRandomValues).
    fn hibedi_random(ptr: *mut u8, len: u32);
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Message {
    Seat { seat: usize },
    State { state: Value },
    Presence { online: bool },
}

/// What the page told us. Every message is a complete snapshot, so only
/// the latest of each kind matters.
#[derive(Default)]
struct Inbox {
    seat: Option<usize>,
    state: Option<Value>,
    online: bool,
}

thread_local! {
    static INBOX: RefCell<Inbox> = RefCell::default();
    static BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Our seat in the page's room; `None` when the page runs a local game.
pub fn session() -> Option<WebSession> {
    INBOX
        .with_borrow(|inbox| inbox.seat)
        .map(|seat| WebSession { seat })
}

pub struct WebSession {
    seat: usize,
}

impl Session for WebSession {
    fn seat(&self) -> usize {
        self.seat
    }

    fn receive(&mut self) -> Option<Value> {
        INBOX.with_borrow_mut(|inbox| inbox.state.take())
    }

    fn publish(&mut self, state: Value) {
        let json = serde_json::to_vec(&state).expect("game states always serialize");
        unsafe { hibedi_send(json.as_ptr(), json.len() as u32) };
    }

    fn everyone_online(&self) -> bool {
        INBOX.with_borrow(|inbox| inbox.online)
    }
}

/// Space for the next message, `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn hibedi_alloc(len: u32) -> *mut u8 {
    BUFFER.with_borrow_mut(|buffer| {
        buffer.clear();
        buffer.resize(len as usize, 0);
        buffer.as_mut_ptr()
    })
}

/// Reads the message the page wrote into the buffer.
#[unsafe(no_mangle)]
pub extern "C" fn hibedi_receive(len: u32) {
    let message = BUFFER.with_borrow(|buffer| serde_json::from_slice(&buffer[..len as usize]));
    INBOX.with_borrow_mut(|inbox| match message {
        Ok(Message::Seat { seat }) => inbox.seat = Some(seat),
        Ok(Message::State { state }) => inbox.state = Some(state),
        Ok(Message::Presence { online }) => inbox.online = online,
        Err(error) => eprintln!("ignoring malformed page message: {error}"),
    });
}

/// Plugin version the miniquad loader checks against the page's plugin
/// registration (web/game.js).
#[unsafe(no_mangle)]
pub extern "C" fn hibedi_crate_version() -> u32 {
    1
}

/// getrandom's custom wasm backend (selected in .cargo/config.toml):
/// entropy comes from the page, which keeps wasm-bindgen out of the
/// build (the miniquad loader cannot provide its imports).
#[unsafe(no_mangle)]
pub extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    unsafe { hibedi_random(dest, len as u32) };
    Ok(())
}
