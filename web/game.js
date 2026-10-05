// Hibe di Hopedi bootstrap: loads the wasm game and wires it to the
// generic lobby. All game-specific glue lives here; lobby.js and the
// connector are reusable for other wasm games.

import { createLobby } from './lobby.js'
import { trysteroConnector } from './connector-trystero.js'

const APP_ID = 'com.github.gobusmax.hibedihopedi'

// --- wasm bridge ---
// The game consumes two entry points (see net/wasm.rs): hibedi_out
// delivers commands, hibedi_random provides entropy; it signals readiness
// by calling hibedi_ready and receives events via hibedi_alloc +
// hibedi_on_event. Events queue here until the module is up.

let wasmExports = null
let connection = null
const pending = []

function push(event) {
  if (!wasmExports) {
    pending.push(event)
    return
  }
  const bytes = new TextEncoder().encode(JSON.stringify(event))
  const pointer = wasmExports.hibedi_alloc(bytes.length)
  new Uint8Array(wasm_memory.buffer).set(bytes, pointer)
  wasmExports.hibedi_on_event(pointer, bytes.length)
}

window.register_plugin = (importObject) => {
  importObject.env.hibedi_out = (pointer, length) => {
    const bytes = new Uint8Array(wasm_memory.buffer).subarray(pointer, pointer + length)
    handleCommand(JSON.parse(new TextDecoder().decode(bytes)))
  }
  importObject.env.hibedi_ready = () => {
    wasmExports = wasm_exports
    for (const event of pending.splice(0)) push(event)
  }
  importObject.env.hibedi_random = (pointer, length) => {
    crypto.getRandomValues(new Uint8Array(wasm_memory.buffer, pointer, length))
  }
}

window.miniquad_add_plugin({
  name: 'hibedi',
  version: 1,
  register_plugin: window.register_plugin,
})

// The wasm filename is content-hashed by web/build.sh and published in
// wasm-name.js, so browsers can cache the blob aggressively: a rebuild
// changes the URL, an unchanged build is served from cache.
window.load(window.WASM_PATH)

// --- lobby wiring ---
// Bridge events: start {role}, leave, peer_joined, peer_left, message.
// Bridge commands: ready, send {data, peer}.

const lobby = createLobby({
  root: document.getElementById('lobby'),
  title: 'Hibe di Hopedi',
  tagline: 'A forced-hop territory prototype',
  connector: trysteroConnector({ appId: APP_ID }),
  onStart(role, roomConnection) {
    connection = roomConnection
    connection.onMessage((peer, data) => push({ type: 'message', peer, data }))
    connection.onPeerJoin((peer) => push({ type: 'peer_joined', peer }))
    connection.onPeerLeave((peer) => push({ type: 'peer_left', peer }))
    push({ type: 'start', role })
  },
  onLeave() {
    push({ type: 'leave' })
  },
  onLocal() {
    push({ type: 'start', role: 'local' })
  },
})

function handleCommand(command) {
  if (command.type === 'ready') {
    lobby.hide()
  } else if (command.type === 'send') {
    connection?.send(command.data, command.peer ?? undefined)
  }
}
