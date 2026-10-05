//! Trystero/WebRTC transport for wasm: a single-threaded bridge to the
//! JS side (web/net.js). The hosting client runs the authoritative
//! [`Session`] locally (see `net::host`); the guest talks to it over the
//! WebRTC data channel. All state lives in one global because the JS
//! entry points have no access to client instances.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Mutex;

use hibedi_core::ClientMessage;

use super::host::HostGame;
use super::{BridgeCommand, BridgeEvent, NetEvent};

unsafe extern "C" {
    /// Called once at startup; the bridge answers with a `booted` event.
    fn hibedi_ready();
    /// Hands an encoded [`BridgeCommand`] to the bridge. The bytes are
    /// only valid for the duration of the call.
    fn hibedi_out(ptr: *const u8, len: u32);
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
                    net.outbound.extend(out);
                }
                Role::Guest => net.outbound.push_back(BridgeCommand::Client { message }),
                Role::Idle => {}
            }
        }
        flush();
    }

    /// Creates a room; the bridge answers with a `room` event.
    pub fn host_room(&self) {
        {
            let mut net = state();
            net.awaiting_room = true;
            net.outbound.push_back(BridgeCommand::Host);
        }
        flush();
    }

    /// Joins an existing room as the guest.
    pub fn join_room(&self, code: String) {
        {
            let mut net = state();
            net.role = Role::Guest;
            net.outbound.push_back(BridgeCommand::Join { code });
        }
        flush();
    }

    pub fn copy(&self, text: String) {
        {
            let mut net = state();
            net.outbound.push_back(BridgeCommand::Copy { text });
        }
        flush();
    }

    /// Leaves the room (if any) and returns to the idle state.
    pub fn leave(&self) {
        {
            let mut net = state();
            net.role = Role::Idle;
            net.awaiting_room = false;
            net.events.clear();
            net.outbound.clear();
            net.outbound.push_back(BridgeCommand::Leave);
        }
        flush();
    }
}

enum Role {
    /// No room: the menu decides what to do.
    Idle,
    /// Joined an existing room as the guest; the host runs the session.
    Guest,
    /// Owns the room and runs the authoritative session locally.
    Host(Box<HostGame>),
}

struct WasmNet {
    role: Role,
    /// Set while waiting for the bridge to confirm a hosted room, so a
    /// stale `room` event cannot hijack the state.
    awaiting_room: bool,
    events: VecDeque<NetEvent>,
    outbound: VecDeque<BridgeCommand>,
}

impl WasmNet {
    const fn new() -> Self {
        Self {
            role: Role::Idle,
            awaiting_room: false,
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
            BridgeEvent::Booted { room } => {
                net.events.push_back(NetEvent::Booted { room });
            }
            BridgeEvent::Copied => net.events.push_back(NetEvent::Copied),
            BridgeEvent::Room { code, share_url } => {
                if net.awaiting_room && matches!(net.role, Role::Idle) {
                    net.awaiting_room = false;
                    let (host, events) = HostGame::new(&mut outbound);
                    net.events.push_back(NetEvent::Room { code, share_url });
                    net.events.extend(events);
                    net.role = Role::Host(Box::new(host));
                }
            }
            BridgeEvent::Joined => {
                if matches!(net.role, Role::Guest) {
                    net.events.push_back(NetEvent::Connected);
                }
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
            BridgeEvent::Client { peer, message } => {
                let events = if let Role::Host(host) = &mut net.role {
                    host.client_message(&peer, message, &mut outbound)
                } else {
                    Vec::new()
                };
                net.events.extend(events);
            }
            BridgeEvent::Server { message } => {
                if matches!(net.role, Role::Guest) {
                    net.events.push_back(NetEvent::Message(message));
                }
            }
        }
    }
    if !outbound.is_empty() {
        state().outbound.extend(outbound);
    }
}
