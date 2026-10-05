// Starts the wasm game and is the page's only contact with it. The
// game runs the same as a local game; online it is additionally handed
// a seat and exchanges game states with the room. The interface, as
// JSON messages (see crates/client/src/web.rs):
//
//   page → game   { type: 'seat', seat }        once, before the game starts
//                 { type: 'state', state }      a game state another player published
//                 { type: 'presence', online }  whether every seat's player is connected
//   game → page   a game state it published
//
// Transport: the miniquad loader's plugin mechanism. The game imports
// hibedi_send (JSON out) and hibedi_random (entropy, for getrandom); the
// page delivers JSON through the exports hibedi_alloc + hibedi_receive.
// Messages sent before the module is instantiated are queued and
// delivered in the loader's on_init, before the game's main runs.

/// `session` is null for a local game, or `{ seat, room }` online.
export function startGame(session) {
  const queued = []
  let exports = null
  let onState = () => {}

  function send(message) {
    if (!exports) {
      queued.push(message)
      return
    }
    const bytes = new TextEncoder().encode(JSON.stringify(message))
    const pointer = exports.hibedi_alloc(bytes.length)
    new Uint8Array(wasm_memory.buffer).set(bytes, pointer)
    exports.hibedi_receive(bytes.length)
  }

  miniquad_add_plugin({
    name: 'hibedi',
    version: 1,
    register_plugin(importObject) {
      importObject.env.hibedi_send = (pointer, length) => {
        const bytes = new Uint8Array(wasm_memory.buffer, pointer, length)
        onState(JSON.parse(new TextDecoder().decode(bytes)))
      }
      importObject.env.hibedi_random = (pointer, length) => {
        crypto.getRandomValues(new Uint8Array(wasm_memory.buffer, pointer, length))
      }
    },
    on_init() {
      exports = wasm_exports
      for (const message of queued.splice(0)) send(message)
    },
  })

  if (session) {
    const { seat, room } = session
    send({ type: 'seat', seat })
    room.onDoc((state) => send({ type: 'state', state }))
    room.onStatus(({ ready }) => send({ type: 'presence', online: ready }))
    onState = (state) => room.publish(state)
  }

  // Hash and pixel ratio make render differences between tabs
  // diagnosable (different hash: stale tab; different ratio: zoom).
  console.log(`hibedi: ${window.WASM_PATH} (devicePixelRatio ${window.devicePixelRatio})`)
  window.load(window.WASM_PATH)
  document.getElementById('glcanvas').focus()
}
