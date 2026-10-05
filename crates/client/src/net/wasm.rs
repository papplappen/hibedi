//! Trystero/WebRTC transport for wasm: a single-threaded bridge to the
//! JS side (web/game.js). Both seats are symmetric: each side plays its
//! local game and exchanges [`StateSync`] announcements through the
//! bridge. The lobby's role only decides who holds the red seat (and so
//! who generates the first game). All state lives in one global because
//! the JS entry points have no access to client instances.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Mutex;

use hibedi_core::{Side, StateSync};

use super::{BridgeCommand, BridgeEvent, NetEvent, StartRole};

unsafe extern "C" {
    /// Called once at startup; the bridge may then push events (and
    /// auto-joins a room when the page URL carries one).
    fn hibedi_ready();
    /// Hands an encoded [`BridgeCommand`] to the bridge. The bytes are
    /// only valid for the duration of the call.
    fn hibedi_out(ptr: *const u8, len: u32);
    /// Fills `ptr[0..len]` with random bytes (crypto.getRandomValues on
    /// the JS side); backs getrandom's custom wasm backend.
    fn hibedi_random(ptr: *mut u8, len: u32);
}

/// getrandom's custom wasm backend hook: routes entropy requests through
/// the JS bridge. This keeps `rand` fully functional on wasm without
/// pulling in wasm-bindgen (which the macroquad loader cannot provide).
#[unsafe(no_mangle)]
pub extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), getrandom::Error> {
    unsafe { hibedi_random(dest, len as u32) };
    Ok(())
}

/// Signals the bridge that the wasm module is ready to receive events.
pub fn bridge_ready() {
    unsafe { hibedi_ready() };
}

#[derive(Clone, Copy, Default)]
pub struct NetHandle;

impl NetHandle {
    pub fn poll(&self) -> Option<NetEvent> {
        let event = state().events.pop_front();
        flush();
        event
    }

    /// Publishes a state announcement to `peer` (the other seat; `None`
    /// broadcasts).
    pub fn announce(&self, sync: &StateSync, peer: Option<&str>) {
        {
            let mut net = state();
            net.outbound.push_back(BridgeCommand::Send {
                data: serde_json::to_value(sync).expect("sync messages always serialize"),
                peer: peer.map(str::to_owned),
            });
        }
        flush();
    }

    /// Tells the bridge the game view is live, so it hides its lobby
    /// overlay and hands the screen to the canvas.
    pub fn notify_ready(&self) {
        {
            state().outbound.push_back(BridgeCommand::Ready);
        }
        flush();
    }

    /// Asks the lobby to return to its menu; the room ends.
    pub fn leave_to_menu(&self) {
        {
            state().outbound.push_back(BridgeCommand::Leave);
        }
        flush();
    }
}

struct WasmNet {
    /// In a room (between the lobby's start and its leave).
    seated: bool,
    /// The peer holding the other seat: the first joiner. Later joiners
    /// are strangers; the lobby shows them the door.
    peer: Option<String>,
    events: VecDeque<NetEvent>,
    outbound: VecDeque<BridgeCommand>,
}

impl WasmNet {
    const fn new() -> Self {
        Self {
            seated: false,
            peer: None,
            events: VecDeque::new(),
            outbound: VecDeque::new(),
        }
    }
}

static NET: Mutex<WasmNet> = Mutex::new(WasmNet::new());

fn state() -> std::sync::MutexGuard<'static, WasmNet> {
    // A poisoned lock would break every later frame; recover instead.
    NET.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Sends every queued command to the bridge. Must not hold the state lock
/// while calling into JS: the bridge may synchronously answer with events
/// (re-entering [`hibedi_on_event`]).
fn flush() {
    let commands: Vec<BridgeCommand> = state().outbound.drain(..).collect();
    for command in commands {
        let json = serde_json::to_vec(&command).expect("bridge commands always serialize");
        unsafe { hibedi_out(json.as_ptr(), json.len() as u32) };
    }
}

// --- JS entry points ---

/// Scratch buffer the bridge writes incoming events into. The pointer is
/// valid until the next call: the bridge uses it alloc→write→notify,
/// with nothing running in between.
#[unsafe(no_mangle)]
pub extern "C" fn hibedi_alloc(len: u32) -> *mut u8 {
    thread_local! {
        static BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
    BUFFER.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        buffer.clear();
        buffer.resize(len as usize, 0);
        buffer.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn hibedi_on_event(ptr: *const u8, len: u32) {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let Ok(event) = serde_json::from_slice::<BridgeEvent>(bytes) else {
        eprintln!("ignoring malformed bridge event");
        return;
    };

    let mut net = state();
    match event {
        BridgeEvent::Start { role } if !net.seated => {
            net.seated = true;
            // The room creator takes red — matching the relay server,
            // which seats the first connection red.
            let side = match role {
                StartRole::Host => Side::Red,
                StartRole::Guest => Side::Green,
                StartRole::Local => {
                    net.seated = false;
                    net.events.push_back(NetEvent::LocalGame);
                    return;
                }
            };
            net.events.push_back(NetEvent::Connected { side });
        }
        BridgeEvent::Start { .. } => {}
        BridgeEvent::Leave => {
            net.seated = false;
            net.peer = None;
            net.events.push_back(NetEvent::Lobby);
        }
        BridgeEvent::PeerJoined { peer } => {
            if net.seated && net.peer.is_none() {
                net.peer = Some(peer.clone());
            }
            net.events.push_back(NetEvent::PeerJoined { peer: Some(peer) });
        }
        BridgeEvent::PeerLeft { peer } => {
            if net.peer.as_deref() == Some(peer.as_str()) {
                // The other seat emptied; a later joiner can reclaim it.
                net.peer = None;
            }
            net.events.push_back(NetEvent::PeerLeft { peer: Some(peer) });
        }
        BridgeEvent::Message { data, .. } => match serde_json::from_value::<StateSync>(data) {
            Ok(sync) => net.events.push_back(NetEvent::Sync(sync)),
            Err(error) => eprintln!("ignoring malformed sync message: {error}"),
        },
    }
}
