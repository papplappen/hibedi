//! Hibe di Hopedi client. The game runs the same everywhere: a local
//! hotseat game, optionally shared through a [`session::Session`].
//! Getting that session is the only thing that differs per platform
//! (see [`start`]).

mod play;
#[cfg(not(target_arch = "wasm32"))]
mod relay;
mod session;
mod ui;
#[cfg(target_arch = "wasm32")]
mod web;

use macroquad::prelude::*;

use play::Match;
use ui::layout::{Layout, PlayArea};

#[macroquad::main(window_conf)]
async fn main() {
    ui::load_fonts();
    #[cfg(not(target_arch = "wasm32"))]
    match_logical_size_on_hidpi();

    let mut play = start().await;
    loop {
        play.update();
        let layout = Layout::compute(play.game());
        handle_input(&mut play, &layout);
        ui::draw(&play, &layout);
        next_frame().await;
    }
}

fn handle_input(play: &mut Match, layout: &Layout) {
    if is_key_pressed(KeyCode::R) {
        play.start_new_game();
    }
    if !is_mouse_button_pressed(MouseButton::Left) {
        return;
    }
    let mouse = mouse_position().into();
    if let Some(PlayArea { board, sidebar }) = &layout.play_area {
        if sidebar.button.contains(mouse) {
            play.start_new_game();
        } else if let Some(coord) = board.coord_at(mouse) {
            play.click(coord);
        }
    }
}

/// The web page decides: it hands us a seat once its room is complete,
/// or none for a local game (web/main.js).
#[cfg(target_arch = "wasm32")]
async fn start() -> Match {
    match web::session() {
        Some(session) => Match::online(Box::new(session)),
        None => Match::local(),
    }
}

/// The command line decides: `--server <url>` plays through a relay,
/// once it has seated a second player.
#[cfg(not(target_arch = "wasm32"))]
async fn start() -> Match {
    match parse_args() {
        Some(url) => Match::online(Box::new(relay::join(url).await)),
        None => Match::local(),
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
                println!("  (no arguments)   local hotseat game");
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

/// Conf sizes are physical window pixels, while the UI is laid out in
/// logical units (macroquad divides by the DPI scale). On hidpi screens,
/// enlarge the window once so the logical workspace matches the
/// 1152×768 design (the browser sizes the canvas in CSS pixels anyway).
#[cfg(not(target_arch = "wasm32"))]
fn match_logical_size_on_hidpi() {
    let scale = macroquad::miniquad::window::dpi_scale();
    if scale > 1.01 {
        macroquad::miniquad::window::set_window_size(
            (1152.0 * scale) as u32,
            (768.0 * scale) as u32,
        );
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Hibe di Hopedi".to_owned(),
        window_width: 1152,
        window_height: 768,
        window_resizable: true,
        // Render at physical resolution on hidpi screens; macroquad keeps
        // coordinates logical and rasterizes glyphs at the higher
        // resolution (crisp text).
        high_dpi: true,
        ..Default::default()
    }
}
