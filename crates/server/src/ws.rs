use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use hibedi_core::{ServerMessage, StateSync};
use tokio::sync::{mpsc, oneshot};

use crate::hub::{Claimed, Command, ConnId};

type Outgoing = futures_util::stream::SplitSink<WebSocket, Message>;

pub fn router(commands: mpsc::UnboundedSender<Command>) -> Router {
    Router::new()
        .route("/", get(websocket_handler))
        .route("/ws", get(websocket_handler))
        .with_state(commands)
}

async fn websocket_handler(
    upgrade: WebSocketUpgrade,
    State(commands): State<mpsc::UnboundedSender<Command>>,
) -> Response {
    upgrade.on_upgrade(move |socket| handle_socket(socket, commands))
}

async fn handle_socket(socket: WebSocket, commands: mpsc::UnboundedSender<Command>) {
    let (mut outgoing, mut incoming) = socket.split();

    // The seat is claimed the moment the connection is upgraded: no
    // hello, no token, the next free slot belongs to whoever connects.
    // The hub answers with the side (already queued into `tx`).
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (reply_tx, reply_rx) = oneshot::channel();
    if commands
        .send(Command::Claim {
            tx: tx.clone(),
            reply: reply_tx,
        })
        .is_err()
    {
        return;
    }

    let conn: ConnId = match reply_rx.await {
        Ok(Claimed::Seated { conn }) => conn,
        Ok(Claimed::Full) => {
            let _ = send(&mut outgoing, &ServerMessage::Full).await;
            return;
        }
        Err(_) => return,
    };

    // Drain outbound relay messages while the read loop below feeds syncs.
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if send(&mut outgoing, &message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = incoming.next().await {
        match message {
            Message::Text(text) => {
                if let Ok(sync) = serde_json::from_str::<StateSync>(text.as_str()) {
                    let _ = commands.send(Command::Sync { from: conn, sync });
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = commands.send(Command::Disconnect { conn });
}

async fn send(outgoing: &mut Outgoing, message: &ServerMessage) -> Result<(), ()> {
    let text = serde_json::to_string(message).expect("relay messages always serialize");
    outgoing
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}
