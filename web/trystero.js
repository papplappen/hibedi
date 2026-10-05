// Trystero transport for room.js: peers in a Trystero room exchanging
// JSON messages over one action. Pinned to an exact version: the action
// API (`send(data, {target})`, `onMessage` property) is 0.26's.
import { joinRoom, selfId } from 'https://cdn.jsdelivr.net/npm/trystero@0.26.0/+esm'

export function trysteroTransport({ appId, roomId }) {
  const room = joinRoom({ appId }, roomId, {
    onJoinError: (details) => console.warn('trystero: join error', details),
  })
  const action = room.makeAction('room')
  return {
    selfId,
    send(message, peer) {
      action
        .send(message, peer ? { target: peer } : undefined)
        .catch((error) => console.warn('trystero: send failed', error))
    },
    onPeerJoin(fn) {
      room.onPeerJoin = fn
    },
    onPeerLeave(fn) {
      room.onPeerLeave = fn
    },
    onMessage(fn) {
      action.onMessage = (data, { peerId }) => fn(peerId, data)
    },
    leave: () => room.leave(),
  }
}
