mod session;
mod ws;

use tokio::sync::mpsc;

const DEFAULT_BIND: &str = "0.0.0.0:3000";

#[tokio::main]
async fn main() {
    let bind = bind_address();

    let (commands, receiver) = mpsc::unbounded_channel();
    tokio::spawn(session::run(receiver));

    let app = ws::router(commands);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|error| panic!("could not bind to {bind}: {error}"));

    println!("hibedi server listening on ws://{bind}/ (and /ws)");
    axum::serve(listener, app)
        .await
        .expect("server terminated unexpectedly");
}

fn bind_address() -> String {
    let mut bind = DEFAULT_BIND.to_owned();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Hibe di Hopedi server");
                println!();
                println!("usage: hibedi-server [--bind <address>]");
                println!();
                println!("  --bind <address>   listen address (default {DEFAULT_BIND})");
                std::process::exit(0);
            }
            "--bind" => {
                bind = args.next().unwrap_or_else(|| {
                    eprintln!("--bind requires an address, e.g. --bind 0.0.0.0:3000");
                    std::process::exit(2);
                });
            }
            other if other.starts_with("--bind=") => {
                bind = other.trim_start_matches("--bind=").to_owned();
            }
            other => {
                eprintln!("unknown argument: {other}");
                eprintln!("run with --help for usage");
                std::process::exit(2);
            }
        }
    }
    bind
}
