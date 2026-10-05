//! Trystero/WebRTC transport for wasm: a single-threaded bridge to the
//! JS side (web/net.js). The hosting client runs the authoritative
//! [`Session`] locally (see `net::host`); the guest talks to it over the
//! WebRTC data channel. All state lives in one global because the JS
//! entry points have no access to client instances.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Mutex;

use hibedi_core::{ClientMessage, ServerMessage};

use super::host::{Directed, HostGame};
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

    pub fn send(&self, message: ClientMessage) {
        {
            let mut net = state();
            match &mut net.role {
                Role::Host(host) => {
                    let mut out = Vec::new();
                    let events = host.local_message(message, &mut out);
                    net.events.extend(events);
                    queue_directed(&mut net.outbound, out);
                }
                Role::Guest => net.outbound.push_back(BridgeCommand::Send {
                    data: serde_json::to_value(&message).expect("client messages always serialize"),
                    peer: None,
                }),
                Role::Idle => {}
            }
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
}

/// Queues directed session messages as generic `send` commands.
fn queue_directed(outbound: &mut VecDeque<BridgeCommand>, messages: Vec<Directed>) {
    outbound.extend(messages.into_iter().map(|(peer, message)| BridgeCommand::Send {
        data: serde_json::to_value(&message).expect("server messages always serialize"),
        peer,
    }));
}

enum Role {
    /// In the lobby: the HTML overlay decides what to do next.
    Idle,
    /// Joined an existing room as the guest; the host runs the session.
    Guest,
    /// Owns the room and runs the authoritative session locally.
    Host(Box<HostGame>),
}

struct WasmNet {
    role: Role,
    events: VecDeque<NetEvent>,
    outbound: VecDeque<BridgeCommand>,
}

impl WasmNet {
    const fn new() -> Self {
        Self {
            role: Role::Idle,
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

    let mut outbound = Vec::new();
    {
        let mut net = state();
        match event {
            BridgeEvent::Start { role } => match role {
                StartRole::Local => net.events.push_back(NetEvent::LocalGame),
                StartRole::Host if matches!(net.role, Role::Idle) => {
                    // The lobby opened a room: run the authoritative session.
                    // Connected goes first so the lobby hands over before
                    // the initial state events arrive.
                    let (host, events) = HostGame::new(&mut outbound);
                    net.events.push_back(NetEvent::Connected);
                    net.events.extend(events);
                    net.role = Role::Host(Box::new(host));
                }
                StartRole::Guest if matches!(net.role, Role::Idle) => {
                    net.role = Role::Guest;
                    net.events.push_back(NetEvent::Connected);
                }
                StartRole::Host | StartRole::Guest => {}
            },
            BridgeEvent::Leave => {
                net.role = Role::Idle;
                net.events.push_back(NetEvent::Lobby);
            }
            BridgeEvent::PeerJoined { peer } => {
                let events = if let Role::Host(host) = &mut net.role {
                    host.peer_joined(&peer, &mut outbound)
                } else {
                    Vec::new()
                };
                net.events.extend(events);
            }
            BridgeEvent::PeerLeft { peer } => {
                let events = match &mut net.role {
                    Role::Host(host) => host.peer_left(&peer, &mut outbound),
                    Role::Guest => vec![NetEvent::Closed {
                        reason: "the host left the room".to_owned(),
                    }],
                    Role::Idle => Vec::new(),
                };
                net.events.extend(events);
            }
            BridgeEvent::Message { peer, data } => match &mut net.role {
                Role::Host(host) => {
                    // Guest→host traffic: client messages.
                    match serde_json::from_value::<ClientMessage>(data) {
                        Ok(message) => {
                            let events = host.client_message(&peer, message, &mut outbound);
                            net.events.extend(events);
                        }
                        Err(error) => eprintln!("ignoring malformed client message: {error}"),
                    }
                }
                Role::Guest => {
                    // Host→guest traffic: server messages.
                    match serde_json::from_value::<ServerMessage>(data) {
                        Ok(message) => net.events.push_back(NetEvent::Message(message)),
                        Err(error) => eprintln!("ignoring malformed server message: {error}"),
                    }
                }
                Role::Idle => {}
            },
        }
    }
    if !outbound.is_empty() {
        let mut net = state();
        queue_directed(&mut net.outbound, outbound);
    }
}
