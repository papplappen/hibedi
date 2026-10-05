// A room of up to `capacity` seated peers sharing one document.
// Game-agnostic: the document is opaque JSON the game publishes; this
// module only decides who holds which seat and which document wins.
// Peers are symmetric: no host, no guest — a room is just a room.
//
// State: two last-writer-wins registers, replicated to every peer.
//
//   seats — the peer ID holding each seat (null: free)
//   doc   — the game's document
//
// Every write carries (version, writer). A higher version wins; equal
// versions (concurrent writes) go to the lower peer ID. All peers apply
// the same rule to the same writes, so they converge on the same state.
//
// Seating is the leader's job: the lowest-ID seated peer we are
// connected to (or we ourselves, when no seated peer is in reach). The
// leader frees the seats of peers who left and seats waiting peers in
// ID order while seats are free. An unseated peer that sees every seat
// held by a connected peer has been refused, and leaves.
//
// A peer touches the seats only once it has heard every connected
// peer's registers: a newcomer that has not yet learned the room's
// state must not overwrite it.
//
// Transport (see trystero.js): { selfId, send(message, peer?),
// onPeerJoin(fn), onPeerLeave(fn), onMessage(fn), leave() }.

const UNWRITTEN = { version: 0, writer: '', value: null }

/// Whether write `a` wins over write `b`.
function wins(a, b) {
  return a.version !== b.version ? a.version > b.version : a.writer < b.writer
}

export function joinRoom(transport, { capacity }) {
  const self = transport.selfId
  const peers = new Set() // connected peers
  const heard = new Set() // connected peers whose registers arrived
  const registers = {
    seats: { ...UNWRITTEN, value: Array(capacity).fill(null) },
    doc: UNWRITTEN,
  }
  const listeners = { status: [], doc: [], refused: [] }
  let refused = false

  const emit = (event, value) => listeners[event].forEach((fn) => fn(value))
  const isPresent = (id) => id === self || peers.has(id)

  function status() {
    const seats = registers.seats.value
    const seat = seats.indexOf(self)
    const occupied = seats.map((id) => id !== null && isPresent(id))
    return {
      seat: seat >= 0 ? seat : null,
      occupied,
      ready: seat >= 0 && occupied.every(Boolean),
    }
  }

  function write(name, value) {
    const { version } = registers[name]
    registers[name] = { version: version + 1, writer: self, value }
    transport.send({ [name]: registers[name] })
  }

  function reconcileSeats() {
    if (![...peers].every((peer) => heard.has(peer))) return

    const seats = registers.seats.value
    const seatedPeers = seats.filter((id) => id !== null && id !== self && peers.has(id))
    const seated = seats.includes(self)
    const leader = seatedPeers.length === 0 || (seated && seatedPeers.every((id) => self < id))

    if (!leader) {
      if (!seated && seats.every((id) => id !== null && isPresent(id))) refuse()
      return
    }

    // Only a seated leader frees the seats of absent peers: a newcomer
    // cannot tell a peer who left from one it has not met yet.
    const next = seated ? seats.map((id) => (id !== null && isPresent(id) ? id : null)) : [...seats]
    for (const id of [self, ...peers].sort()) {
      const free = next.indexOf(null)
      if (free < 0) break
      if (!next.includes(id)) next[free] = id
    }
    if (next.some((id, seat) => id !== seats[seat])) write('seats', next)
  }

  function refuse() {
    refused = true
    transport.leave()
    emit('refused')
  }

  function update() {
    if (refused) return
    reconcileSeats()
    if (!refused) emit('status', status())
  }

  transport.onPeerJoin((peer) => {
    peers.add(peer)
    transport.send(registers, peer)
    update()
  })

  transport.onPeerLeave((peer) => {
    peers.delete(peer)
    heard.delete(peer)
    update()
  })

  transport.onMessage((peer, message) => {
    heard.add(peer)
    for (const name of ['seats', 'doc']) {
      const incoming = message?.[name]
      if (incoming && wins(incoming, registers[name])) {
        registers[name] = incoming
        if (name === 'doc') emit('doc', incoming.value)
      }
    }
    update()
  })

  // Alone so far: seat ourselves (we are our own leader).
  queueMicrotask(update)

  return {
    /// `fn(status)` now and on every update: `seat` (ours, or null),
    /// `occupied` (per seat: held by a connected peer), and `ready`
    /// (we are seated and every seat is occupied).
    onStatus(fn) {
      listeners.status.push(fn)
      fn(status())
    },
    /// `fn(doc)` with the current document, if any, and whenever
    /// another peer's document wins.
    onDoc(fn) {
      listeners.doc.push(fn)
      if (registers.doc.value !== null) fn(registers.doc.value)
    },
    /// `fn()` once if every seat is taken; the room is left already.
    onRefused(fn) {
      listeners.refused.push(fn)
    },
    /// Replaces the shared document with ours.
    publish(doc) {
      write('doc', doc)
    },
    leave() {
      transport.leave()
    },
  }
}
