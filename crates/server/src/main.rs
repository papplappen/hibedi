mod hub;
mod ws;

use tokio::sync::mpsc;

const DEFAULT_BIND: &str = "0.0.0.0:3000";
/// Hibe di Hopedi is a two-player game; the relay itself is generic.
const DEFAULT_SEATS: usize = 2;

struct Args {
    bind: String,
    seats: usize,
}

#[tokio::main]
async fn main() {
    let Args { bind, seats } = parse_args();

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

fn parse_args() -> Args {
    let mut args = Args {
        bind: DEFAULT_BIND.to_owned(),
        seats: DEFAULT_SEATS,
    };
    let mut raw = std::env::args().skip(1);
    while let Some(arg) = raw.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
            None => (arg, None),
        };
        let mut value = |example: &str| {
            inline.clone().or_else(|| raw.next()).unwrap_or_else(|| {
                eprintln!("{flag} requires a value, e.g. {flag} {example}");
                std::process::exit(2);
            })
        };
        match flag.as_str() {
            "--help" | "-h" => {
                println!("Hibe di Hopedi relay server");
                println!();
                println!("usage: hibedi-server [--bind <address>] [--seats <n>]");
                println!();
                println!("  --bind <address>   listen address (default {DEFAULT_BIND})");
                println!("  --seats <n>        players per room (default {DEFAULT_SEATS})");
                std::process::exit(0);
            }
            "--bind" => args.bind = value("0.0.0.0:3000"),
            "--seats" => {
                args.seats = value("2")
                    .parse()
                    .ok()
                    .filter(|&n| n > 0)
                    .unwrap_or_else(|| {
                        eprintln!("--seats requires a positive number");
                        std::process::exit(2);
                    })
            }
            other => {
                eprintln!("unknown argument: {other}");
                eprintln!("run with --help for usage");
                std::process::exit(2);
            }
        }
    }
    args
}
