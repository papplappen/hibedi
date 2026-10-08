//! Hibe di Hopedi client. The game runs the same everywhere: a local
//! hotseat game, optionally shared through a [`session::Session`].
//! Getting that session is the only thing that differs per platform
//! (see [`start`]).
//!
//! - [`play`]: the match, the one resource the screen is drawn from,
//! - [`settings`]: the player's settings,
//! - [`ui`]: everything on screen, and turning clicks into moves.

#![allow(
    clippy::type_complexity,
    reason = "Bevy system parameters spell out their queries"
)]

mod play;
#[cfg(not(target_arch = "wasm32"))]
mod relay;
mod session;
mod settings;
mod storage;
mod ui;
#[cfg(target_arch = "wasm32")]
mod web;

use bevy::prelude::*;

use play::Match;
use settings::Settings;

fn main() -> AppExit {
    let settings = Settings::load();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window()),
        ..default()
    }))
    .add_plugins((play::plugin, settings::plugin, ui::plugin));
    start(&mut app, &settings);
    app.insert_resource(settings);
    app.run()
}

/// The web page decides: it hands us a seat once its room is complete,
/// or none for a local game (web/main.js).
#[cfg(target_arch = "wasm32")]
fn start(app: &mut App, settings: &Settings) {
    app.insert_resource(match web::session() {
        Some(session) => Match::online(Box::new(session), settings),
        None => Match::local(settings),
    });
}

/// The command line decides: `--server <url>` plays through a relay,
/// once it has seated a second player.
#[cfg(not(target_arch = "wasm32"))]
fn start(app: &mut App, settings: &Settings) {
    match parse_args() {
        Some(url) => relay::join(app, url),
        None => {
            app.insert_resource(Match::local(settings));
        }
    }
}

/// Returns the relay URL, if one was given.
#[cfg(not(target_arch = "wasm32"))]
fn parse_args() -> Option<String> {
    let mut server = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Hibe di Hopedi");
                println!();
                println!("usage: hibedi-client [--server <ws-url>]");
                println!();
                println!("  --server <url>   play online through a relay server (ws://host:3000)");
                println!("  (no arguments)   a game on this device (see its settings)");
                std::process::exit(0);
            }
            "--server" => {
                server = Some(args.next().unwrap_or_else(|| {
                    eprintln!("--server requires a URL, e.g. --server ws://127.0.0.1:3000");
                    std::process::exit(2);
                }));
            }
            other if other.starts_with("--server=") => {
                server = Some(other.trim_start_matches("--server=").to_owned());
            }
            other => {
                eprintln!("unknown argument: {other}");
                eprintln!("run with --help for usage");
                std::process::exit(2);
            }
        }
    }
    server.map(|raw| normalize_url(&raw))
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("wss://") {
        eprintln!("TLS (wss://) is not supported by this prototype; use ws://");
        std::process::exit(2);
    }
    if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("ws://{trimmed}")
    }
}

fn window() -> Window {
    Window {
        title: "Hibe di Hopedi".to_owned(),
        resolution: (1152, 768).into(),
        // On the web: the page's canvas, filling the body (page.css).
        canvas: Some("#glcanvas".to_owned()),
        fit_canvas_to_parent: true,
        ..default()
    }
}
