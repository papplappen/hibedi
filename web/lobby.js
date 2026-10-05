// Generic lobby for wasm games: host a room, join one by code, or play
// locally. Game-agnostic — it establishes a connection through the
// pluggable connector and hands it to `onStart`; it knows nothing about
// the game itself. Styling lives in lobby.css, the mount point is the
// `root` element.
//
// Teardown policy: a guest whose peer disappears is returned to the menu
// (a two-player room without its host is dead); a host keeps the room up
// so the seat can be reclaimed.

const CODE_ALPHABET = '23456789ABCDEFGHJKMNPQRSTUVWXYZ'
const CODE_LENGTH = 5

export function createLobby({ root, title, tagline, connector, onStart, onLeave, onLocal }) {
  let connection = null
  let role = null
  let code = null

  root.classList.add('lobby')

  root.innerHTML = `
    <div class="lobby-card">
      <h1></h1>
      <p class="tagline"></p>
      <p class="notice" hidden></p>

      <section data-view="menu">
        <button data-action="host" class="primary">Host game</button>
        <p class="hint">create a room and share the link</p>
        <button data-action="join-view" class="secondary">Join with code</button>
        <p class="hint">enter a code from your friend</p>
        <button data-action="local" class="secondary">Play locally</button>
        <p class="hint">two players on this device</p>
        <p class="footer">online play is peer-to-peer: the first player hosts the room.</p>
      </section>

      <section data-view="join" hidden>
        <h2>Join a room</h2>
        <input data-ref="code-input" maxlength="8" autocomplete="off"
               autocapitalize="characters" spellcheck="false" placeholder="room code">
        <div class="row">
          <button data-action="join" class="primary">Join</button>
          <button data-action="back" class="secondary">Back</button>
        </div>
      </section>

      <section data-view="hosting" hidden>
        <h2>Waiting for a second player</h2>
        <p class="code" data-ref="code-display"></p>
        <button data-action="copy" class="primary">Copy invite link</button>
        <a data-ref="share-link" class="share-link" target="_blank" rel="noopener"></a>
        <button data-action="back" class="secondary">Back</button>
      </section>

      <section data-view="joining" hidden>
        <h2>Joining <span data-ref="joining-code"></span></h2>
        <p class="hint" data-ref="joining-status">connecting…</p>
        <button data-action="back" class="secondary">Back</button>
      </section>
    </div>`

  root.querySelector('h1').textContent = title
  root.querySelector('.tagline').textContent = tagline

  const views = {}
  for (const section of root.querySelectorAll('section[data-view]')) {
    views[section.dataset.view] = section
  }
  const ref = (name) => root.querySelector(`[data-ref="${name}"]`)
  const refs = {
    notice: root.querySelector('.notice'),
    codeInput: ref('code-input'),
    codeDisplay: ref('code-display'),
    shareLink: ref('share-link'),
    joiningCode: ref('joining-code'),
    joiningStatus: ref('joining-status'),
    copyButton: root.querySelector('[data-action="copy"]'),
  }

  function show(view) {
    for (const [name, section] of Object.entries(views)) {
      section.hidden = name !== view
    }
  }

  function notice(text) {
    refs.notice.textContent = text ?? ''
    refs.notice.hidden = !text
  }

  function hide() {
    root.classList.add('hidden')
  }

  function host() {
    code = generateCode()
    role = 'host'
    connection = connector.host(code)
    onStart(role, connection)
    watch()
    refs.codeDisplay.textContent = code
    refs.shareLink.href = shareUrl(code)
    refs.shareLink.textContent = shareUrl(code)
    show('hosting')
  }

  function join(raw) {
    const cleaned = sanitize(raw)
    if (!cleaned) return
    code = cleaned
    role = 'guest'
    connection = connector.join(code)
    onStart(role, connection)
    watch()
    refs.joiningCode.textContent = code
    refs.joiningStatus.textContent = 'waiting for the host…'
    show('joining')
  }

  // The game's handlers are registered first (in onStart), so it sees
  // every event before the lobby's teardown policy acts on it.
  function watch() {
    connection.onPeerJoin(() => {
      if (role === 'guest') {
        refs.joiningStatus.textContent = 'connected — waiting for the host to start…'
      }
    })
    connection.onPeerLeave(() => {
      if (role === 'guest') {
        back('the host left the room')
      }
    })
  }

  function back(leaveNotice = null) {
    leaveRoom()
    notice(leaveNotice)
    show('menu')
    onLeave()
  }

  function leaveRoom() {
    connection?.leave()
    connection = null
    role = null
    code = null
    // Drop the deep link so a refresh does not rejoin a dead room.
    const url = new URL(window.location.href)
    if (url.searchParams.has('room')) {
      url.searchParams.delete('room')
      history.replaceState(null, '', url.pathname + url.search)
    }
  }

  async function copyInvite() {
    try {
      await navigator.clipboard.writeText(shareUrl(code))
      refs.copyButton.textContent = 'Copied!'
      setTimeout(() => {
        refs.copyButton.textContent = 'Copy invite link'
      }, 1500)
    } catch {
      // Clipboard unavailable: the link is on screen for manual copying.
    }
  }

  root.addEventListener('click', (event) => {
    const action = event.target.closest('[data-action]')?.dataset.action
    if (action === 'host') host()
    else if (action === 'join-view') {
      notice(null)
      show('join')
      refs.codeInput.focus()
    } else if (action === 'join') join(refs.codeInput.value)
    else if (action === 'local') {
      onLocal()
      hide()
    } else if (action === 'back') back()
    else if (action === 'copy') copyInvite()
  })

  refs.codeInput.addEventListener('input', () => {
    refs.codeInput.value = refs.codeInput.value.toUpperCase()
  })
  refs.codeInput.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') join(refs.codeInput.value)
  })

  window.addEventListener('beforeunload', () => connection?.leave())

  // Deep link: ?room=CODE joins right away.
  const fromUrl = sanitize(new URLSearchParams(window.location.search).get('room'))
  if (fromUrl) join(fromUrl)

  return { hide }
}

function generateCode() {
  const bytes = new Uint8Array(CODE_LENGTH)
  crypto.getRandomValues(bytes)
  let code = ''
  for (const byte of bytes) code += CODE_ALPHABET[byte % CODE_ALPHABET.length]
  return code
}

function sanitize(raw) {
  const cleaned = (raw ?? '')
    .toUpperCase()
    // The alphabet excludes 0/1/I/L/O as visually ambiguous.
    .replace(/[^2-9A-HJ-KM-NP-Z]/g, '')
  return cleaned.length > 0 ? cleaned : null
}

function shareUrl(code) {
  const url = new URL(window.location.href)
  url.searchParams.set('room', code)
  return url.toString()
}
