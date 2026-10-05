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
use hibedi_core::{ClientMessage, ServerMessage};
use tokio::sync::{mpsc, oneshot};

use crate::session::{ClaimOutcome, Command};

type Outgoing = futures_util::stream::SplitSink<WebSocket, Message>;
type Incoming = futures_util::stream::SplitStream<WebSocket>;

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

    let Some(hello) = next_text(&mut incoming).await else {
        return;
    };
    let Ok(ClientMessage::Hello { token }) = serde_json::from_str::<ClientMessage>(&hello) else {
        let _ = send(
            &mut outgoing,
            &ServerMessage::Error {
                reason: "first message must be hello".to_owned(),
            },
        )
        .await;
        return;
    };

    let (tx, mut rx) = mpsc::unbounded_channel();
    let (reply_tx, reply_rx) = oneshot::channel();
    if commands
        .send(Command::Claim {
            token,
            tx: tx.clone(),
            reply: reply_tx,
        })
        .is_err()
    {
        return;
    }

    let outcome = match reply_rx.await {
        Ok(outcome) => outcome,
        Err(_) => return,
    };

    let side = match outcome {
        ClaimOutcome::Full => {
            let _ = send(
                &mut outgoing,
                &ServerMessage::Error {
                    reason: "both seats are taken".to_owned(),
                },
            )
            .await;
            return;
        }
        ClaimOutcome::Seated { side, token } => {
            let _ = tx.send(ServerMessage::Welcome {
                side,
                token: token.clone(),
            });
            let _ = commands.send(Command::Sync);
            side
        }
    };

    // Drain outbound state messages while the read loop below feeds commands.
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if send(&mut outgoing, &message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = incoming.next().await {
        match message {
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(text.as_str()) {
                Ok(client_message) => {
                    let _ = commands.send(Command::Message {
                        side,
                        msg: client_message,
                    });
                }
                Err(_) => {
                    let _ = tx.send(ServerMessage::Error {
                        reason: "malformed message".to_owned(),
                    });
                }
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = commands.send(Command::Disconnect { side, tx });
}

async fn next_text(incoming: &mut Incoming) -> Option<String> {
    while let Some(Ok(message)) = incoming.next().await {
        match message {
            Message::Text(text) => return Some(text.to_string()),
            Message::Close(_) => return None,
            _ => {}
        }
    }
    None
}

async fn send(outgoing: &mut Outgoing, message: &ServerMessage) -> Result<(), ()> {
    let text = serde_json::to_string(message).expect("server messages always serialize");
    outgoing
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}
