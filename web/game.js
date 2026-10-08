// Starts the wasm game and is the page's only contact with it. The
// game runs the same as a local game; online it is additionally handed
// a seat and exchanges game states with the room
// (see crates/client/src/web.rs). The game asks the global `hibedi`:
//
//   seat()       our seat; null for a local game
//   takeState()  the latest game state another player published, as
//                JSON, if one arrived since the last call; else null
//   online()     whether every seat's player is connected
//   publish(s)   shares a game state the game published (JSON)
//   load(k)      the text stored under key k, or null
//   save(k, s)   stores text s under key k
//   download(n, s)  offers text s as a file named n
//
// The page keeps what the room sends until the game asks, so a game in
// progress is waiting for the game when it starts.

/// `session` is null for a local game, or `{ seat, room }` online.
export async function startGame(session) {
  let state = null
  let online = false
  session?.room.onDoc((doc) => (state = JSON.stringify(doc)))
  session?.room.onStatus(({ ready }) => (online = ready))

  globalThis.hibedi = {
    seat: () => (session ? session.seat : null),
    takeState() {
      const taken = state
      state = null
      return taken
    },
    online: () => online,
    publish: (json) => session?.room.publish(JSON.parse(json)),
    // Storage may be unavailable (private browsing, blocked cookies):
    // then nothing is kept.
    load(key) {
      try {
        return localStorage.getItem(`hibedi.${key}`)
      } catch {
        return null
      }
    },
    save(key, text) {
      try {
        localStorage.setItem(`hibedi.${key}`, text)
      } catch {}
    },
    download(name, text) {
      const link = document.createElement('a')
      link.href = URL.createObjectURL(new Blob([text], { type: 'text/plain' }))
      link.download = name
      link.click()
      setTimeout(() => URL.revokeObjectURL(link.href), 1000)
    },
  }

  // Build names and pixel ratio make render differences between tabs
  // diagnosable (different build: stale tab; different ratio: zoom).
  const { glue, wasm } = window.HIBEDI_BUILD
  console.log(`hibedi: ${wasm} (devicePixelRatio ${window.devicePixelRatio})`)
  const game = await import(glue)
  await game.default({ module_or_path: wasm })
  document.getElementById('glcanvas').focus()
}
