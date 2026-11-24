// The page: menu, rooms, and starting the game. The URL is the whole
// navigation state, and every move between states is a page load:
//
//   (no query)    the menu
//   ?room=CODE    the room CODE: wait for the other player, then play
//   ?local        a game on this device
//
// so the browser's Back button returns to the menu, and a shared link
// is an invitation. The game itself (game.js) only starts once the room
// is complete.

import { joinRoom } from './room.js'
import { startGame } from './game.js'
import { trysteroTransport } from './trystero.js'

const APP_ID = 'com.github.papplappen.hibedi'
/// Hibedi seats two players: red and green.
const SEATS = 2

// No 0/1/I/L/O: codes are read aloud and typed from screens.
const CODE_ALPHABET = '23456789ABCDEFGHJKMNPQRSTUVWXYZ'
const CODE_LENGTH = 5

const page = document.getElementById('page')
const ref = (name) => page.querySelector(`[data-ref="${name}"]`)

const params = new URLSearchParams(location.search)
const code = sanitizeCode(params.get('room'))
if (code) {
  enterRoom(code)
} else if (params.has('local')) {
  page.hidden = true
  startGame(null)
} else {
  showMenu()
}

function show(view, notice = null) {
  for (const element of page.querySelectorAll('[data-view]')) {
    element.hidden = element.dataset.view !== view
  }
  ref('notice').textContent = notice ?? ''
  ref('notice').hidden = !notice
}

function showMenu(notice = null) {
  show('menu', notice)
  ref('create').href = roomUrl(generateCode())
  ref('join').addEventListener('submit', (event) => {
    event.preventDefault()
    const code = sanitizeCode(event.target.elements.room.value)
    if (code) location.assign(roomUrl(code))
    else show('menu', 'Enter the room code you were given.')
  })
}

function enterRoom(code) {
  show('room')
  document.title = `${code} · ${document.title}`
  ref('code').textContent = code
  const link = ref('link')
  link.value = roomUrl(code)
  link.addEventListener('focus', () => link.select())
  ref('copy').addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(link.value)
      ref('copy').textContent = 'Copied'
    } catch {
      link.focus() // Clipboard unavailable: select the link for manual copying.
    }
  })

  const room = joinRoom(trysteroTransport({ appId: APP_ID, roomId: code }), { capacity: SEATS })
  room.onRefused(() => {
    history.replaceState(null, '', './')
    showMenu(`Room ${code} is full.`)
  })
  let started = false
  room.onStatus(({ seat, ready }) => {
    if (started) return
    if (ready) {
      started = true
      page.hidden = true
      startGame({ seat, room })
    } else {
      ref('room-status').textContent =
        seat === null ? 'Connecting to the room…' : 'Waiting for the other player…'
    }
  })

  addEventListener('pagehide', () => room.leave())
  // Restored from the back/forward cache: the room was left on hide.
  addEventListener('pageshow', (event) => {
    if (event.persisted) location.reload()
  })
}

function roomUrl(code) {
  const url = new URL('./', location.href)
  url.searchParams.set('room', code)
  return url.href
}

function generateCode() {
  const bytes = crypto.getRandomValues(new Uint8Array(CODE_LENGTH))
  return Array.from(bytes, (byte) => CODE_ALPHABET[byte % CODE_ALPHABET.length]).join('')
}

/// A typed code reduced to the code alphabet, or null if nothing remains.
function sanitizeCode(raw) {
  const code = [...(raw ?? '').toUpperCase()].filter((c) => CODE_ALPHABET.includes(c)).join('')
  return code || null
}
