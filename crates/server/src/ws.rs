//! WebSocket front of the relay: one task per connection, translating
//! between the wire protocol and the room's commands.

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
use hibedi_relay_protocol::{ClientMessage, ServerMessage};
use tokio::sync::{mpsc, oneshot};

use crate::hub::Command;

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

    // The client opens with a hello naming the seat it wants.
    let wanted = loop {
        match incoming.next().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str(text.as_str()) {
                Ok(ClientMessage::Hello { seat }) => break seat,
                _ => return,
            },
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            _ => return,
        }
    };

    let (tx, mut rx) = mpsc::unbounded_channel();
    let (reply, seated) = oneshot::channel();
    if commands.send(Command::Join { tx, wanted, reply }).is_err() {
        return;
    }
    let Ok(Some(seat)) = seated.await else {
        let _ = send(&mut outgoing, &ServerMessage::Full).await;
        let _ = outgoing.close().await;
        return;
    };

    // The room's messages for this seat; ends once the room drops the
    // seat's sender after `Leave`.
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if send(&mut outgoing, &message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = incoming.next().await {
        match message {
            Message::Text(text) => match serde_json::from_str(text.as_str()) {
                Ok(ClientMessage::Offer { version, state }) => {
                    let _ = commands.send(Command::Offer {
                        seat,
                        version,
                        state,
                    });
                }
                Ok(ClientMessage::Hello { .. }) => {}
                Err(error) => eprintln!("seat {seat}: ignoring malformed message: {error}"),
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = commands.send(Command::Leave { seat });
}

async fn send(outgoing: &mut Outgoing, message: &ServerMessage) -> Result<(), axum::Error> {
    let text = serde_json::to_string(message).expect("relay messages always serialize");
    outgoing.send(Message::Text(text.into())).await
}
