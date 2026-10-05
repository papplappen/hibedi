// Trystero adapter: normalizes a Trystero room into the minimal
// connection interface the lobby hands to a game:
//
//   send(data, targetPeer?)  — `targetPeer` aims at one peer,
//                              undefined broadcasts
//   leave()
//   onPeerJoin(fn) / onPeerLeave(fn) / onMessage(fn)
//                            — subscribe; handlers may be added freely
//
// Wire format: two Trystero actions carrying opaque JSON-serializable
// payloads — 'client' flows guest → host, 'server' flows host → guest.
import { joinRoom } from 'https://esm.run/trystero'

export function trysteroConnector({ appId }) {
  const config = { appId }
  return {
    host: (code) => connect(config, code, true),
    join: (code) => connect(config, code, false),
  }
}

function connect(config, code, isHost) {
  const room = joinRoom(config, code)
  const client = room.makeAction('client')
  const server = room.makeAction('server')
  const listeners = {
    peerJoin: new Set(),
    peerLeave: new Set(),
    message: new Set(),
  }

  const connection = {
    send(data, targetPeer) {
      ;(isHost ? server : client).send(
        data,
        targetPeer ? { target: targetPeer } : undefined,
      )
    },
    leave: () => room.leave(),
    onPeerJoin: (fn) => listeners.peerJoin.add(fn),
    onPeerLeave: (fn) => listeners.peerLeave.add(fn),
    onMessage: (fn) => listeners.message.add(fn),
  }

  const deliver = (fnName, ...args) => {
    for (const fn of listeners[fnName]) fn(...args)
  }
  for (const action of [client, server]) {
    action.onMessage = (data, { peerId }) => deliver('message', peerId, data)
  }
  room.onPeerJoin = (peerId) => deliver('peerJoin', peerId)
  room.onPeerLeave = (peerId) => deliver('peerLeave', peerId)
  return connection
}
