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
    let mut settings = Settings::load();
    let args = parse_args(&mut settings);
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window()),
        ..default()
    }))
    .add_plugins((play::plugin, settings::plugin, ui::plugin));
    start(&mut app, args, &settings);
    app.insert_resource(settings);
    app.run()
}

/// The web page takes no arguments.
#[cfg(target_arch = "wasm32")]
struct Args;

#[cfg(target_arch = "wasm32")]
fn parse_args(_: &mut Settings) -> Args {
    Args
}

/// The web page decides: it hands us a seat once its room is complete,
/// or none for a local game (web/main.js).
#[cfg(target_arch = "wasm32")]
fn start(app: &mut App, _: Args, settings: &Settings) {
    app.insert_resource(match web::session() {
        Some(session) => Match::online(Box::new(session), settings),
        None => Match::local(settings),
    });
}

/// Hibe di Hopedi: a game on this device, against someone here or a
/// bot (see the settings), or online through a relay server.
#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Parser)]
#[command(name = "hibedi-client")]
struct Args {
    /// Play online through a relay server, e.g. ws://host:3000
    #[arg(long, value_name = "URL", value_parser = relay_url, conflicts_with_all = ["game", "seed"])]
    server: Option<String>,
    /// Continue a game written in notation, on this device
    #[arg(long, value_name = "FILE", value_parser = read_game)]
    game: Option<hibedi_core::Game>,
    /// Deal the same boards every time, on this device
    #[arg(long)]
    seed: Option<u64>,
    #[command(
        flatten,
        next_help_heading = "Settings for this run (saved only along with a change in the game)"
    )]
    settings: settings::SettingsArgs,
}

/// Reads the command line, applying the settings given on it.
#[cfg(not(target_arch = "wasm32"))]
fn parse_args(settings: &mut Settings) -> Args {
    let mut args = <Args as clap::Parser>::parse();
    std::mem::take(&mut args.settings).apply(settings);
    args
}

/// The command line decides: a relay server, or a game on this device.
#[cfg(not(target_arch = "wasm32"))]
fn start(app: &mut App, args: Args, settings: &Settings) {
    match args.server {
        Some(url) => relay::join(app, url),
        None => {
            let play = Match::local(settings)
                .with_seed(args.seed)
                .with_game(args.game);
            app.insert_resource(play);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn relay_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("wss://") {
        return Err("TLS (wss://) is not supported by this prototype; use ws://".to_owned());
    }
    Ok(if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("ws://{trimmed}")
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn read_game(path: &str) -> Result<hibedi_core::Game, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    hibedi_core::notation::read(&text).map_err(|error| error.to_string())
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
