mod hub;
mod ws;

use std::num::NonZeroUsize;

use clap::Parser;
use tokio::sync::mpsc;

/// Hibe di Hopedi relay server: seats players in rooms and syncs the
/// game state between them.
#[derive(Parser)]
#[command(name = "hibedi-server")]
struct Args {
    /// Address to listen on
    #[arg(long, default_value = "0.0.0.0:3000")]
    bind: String,
    /// Players per room (Hibe di Hopedi seats two; the relay is generic)
    #[arg(long, default_value = "2")]
    seats: NonZeroUsize,
}

#[tokio::main]
async fn main() {
    let Args { bind, seats } = Args::parse();
    let seats = seats.get();

    let (commands, receiver) = mpsc::unbounded_channel();
    tokio::spawn(hub::run(seats, receiver));

    let app = ws::router(commands);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|error| panic!("could not bind to {bind}: {error}"));

    println!("hibedi relay listening on ws://{bind}/ (and /ws), {seats} seats");
    axum::serve(listener, app)
        .await
        .expect("server terminated unexpectedly");
}
