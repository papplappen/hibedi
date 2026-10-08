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
    clippy::too_many_arguments,
    reason = "Bevy systems take everything they use as parameters, queries spelled out"
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
/// once it has seated a second player; `--game <file>` continues a game
/// written in notation (see [`hibedi_core::notation`]) on this device,
/// and `--seed <n>` deals the same boards every time.
#[cfg(not(target_arch = "wasm32"))]
fn start(app: &mut App, settings: &Settings) {
    match parse_args() {
        Args::Online(url) => relay::join(app, url),
        Args::Local { game, seed } => {
            let play = Match::local(settings)
                .with_seed(seed)
                .with_game(game.map(|game| *game));
            app.insert_resource(play);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum Args {
    Online(String),
    /// A game on this device, continuing the game given, if any, and
    /// dealing from the seed given, if any.
    Local {
        game: Option<Box<hibedi_core::Game>>,
        seed: Option<u64>,
    },
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_args() -> Args {
    let fail = |message: String| -> ! {
        eprintln!("{message}");
        eprintln!("run with --help for usage");
        std::process::exit(2);
    };
    let mut server = None;
    let mut game = None;
    let mut seed = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
            None => (arg.clone(), None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| args.next())
                .unwrap_or_else(|| fail(format!("{flag} requires a value")))
        };
        match flag.as_str() {
            "--help" | "-h" => {
                println!("Hibe di Hopedi");
                println!();
                println!("usage: hibedi-client [--server <ws-url> | --game <file> | --seed <n>]");
                println!();
                println!("  --server <url>   play online through a relay server (ws://host:3000)");
                println!("  --game <file>    continue a game written in notation, on this device");
                println!("  --seed <n>       deal the same boards every time, on this device");
                println!("  (no arguments)   a game on this device (see its settings)");
                std::process::exit(0);
            }
            "--server" => server = Some(normalize_url(&value())),
            "--game" => {
                let path = value();
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| fail(format!("cannot read {path}: {error}")));
                game = Some(Box::new(
                    hibedi_core::notation::read(&text)
                        .unwrap_or_else(|error| fail(format!("{path}: {error}"))),
                ));
            }
            "--seed" => {
                let text = value();
                seed = Some(
                    text.parse()
                        .unwrap_or_else(|_| fail(format!("not a seed: {text}"))),
                );
            }
            other => fail(format!("unknown argument: {other}")),
        }
    }
    match server {
        Some(_) if game.is_some() || seed.is_some() => {
            fail("--game and --seed are for games on this device, not online".to_owned())
        }
        Some(url) => Args::Online(url),
        None => Args::Local { game, seed },
    }
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
