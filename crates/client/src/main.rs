mod net;

use std::sync::OnceLock;

use hibedi_core::{
    BOARD_HEIGHT, BOARD_WIDTH, Cell, Coord, Game, GameResult, Phase, Replica, Side, StateSync,
    WinReason,
};
use macroquad::prelude::*;
use net::{NetEvent, NetHandle};

const BACKGROUND: Color = Color::from_rgba(17, 21, 30, 255);
const PANEL: Color = Color::from_rgba(31, 38, 51, 255);
const PANEL_EDGE: Color = Color::from_rgba(67, 77, 96, 255);
const EMPTY_CELL: Color = Color::from_rgba(228, 224, 210, 255);
const RED_CELL: Color = Color::from_rgba(211, 79, 76, 255);
const GREEN_CELL: Color = Color::from_rgba(53, 158, 108, 255);
const USED_CELL_OVERLAY: Color = Color::from_rgba(43, 48, 57, 185);
const TOKEN_FILL: Color = Color::from_rgba(24, 31, 43, 255);
const TOKEN_EDGE: Color = Color::from_rgba(239, 236, 220, 255);
const GOLD: Color = Color::from_rgba(194, 137, 38, 255);
const CYAN: Color = Color::from_rgba(33, 139, 154, 255);
/// Outline marking the viewer's own cells (online only).
const OWN_EDGE: Color = Color::from_rgba(245, 242, 232, 255);
const MUTED: Color = Color::from_rgba(169, 178, 191, 255);

/// Screen padding around the whole play area.
const PAD: f32 = 24.0;
/// Height of the header band (title + turn chip) above the play area.
const HEAD: f32 = 64.0;
/// Gap between the board and the info panel.
const GAP: f32 = 24.0;
/// Info panel width when it sits beside the board.
const PANEL_W: f32 = 280.0;
/// Info panel height; the content layout below is designed for this.
const PANEL_H: f32 = 248.0;
/// A side panel narrower than this is not worth the board width it
/// costs; stack the panel below the board instead.
const MIN_PANEL_W: f32 = 220.0;

/// Embedded UI fonts (SIL OFL — see `assets/OFL.txt`). Atkinson
/// Hyperlegible is a legibility-first typeface; swap the files to change
/// the look. Unlike the built-in font these rasterize anti-aliased at
/// every size.
struct Ui {
    regular: Font,
    bold: Font,
}

static UI: OnceLock<Ui> = OnceLock::new();

impl Ui {
    fn load() -> Self {
        Self {
            regular: load_ttf_font_from_bytes(include_bytes!(
                "../assets/AtkinsonHyperlegible-Regular.ttf"
            ))
            .expect("embedded font parses"),
            bold: load_ttf_font_from_bytes(include_bytes!(
                "../assets/AtkinsonHyperlegible-Bold.ttf"
            ))
            .expect("embedded font parses"),
        }
    }
}

fn ui() -> &'static Ui {
    UI.get().expect("ui fonts are loaded in main")
}

fn text(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(&ui().regular),
            font_size: size as u16,
            color,
            ..Default::default()
        },
    );
}

fn bold_text(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font: Some(&ui().bold),
            font_size: size as u16,
            color,
            ..Default::default()
        },
    );
}

fn text_width(text: &str, size: f32) -> f32 {
    measure_text(text, Some(&ui().regular), size as u16, 1.0).width
}

fn bold_text_width(text: &str, size: f32) -> f32 {
    measure_text(text, Some(&ui().bold), size as u16, 1.0).width
}

struct BoardLayout {
    x: f32,
    y: f32,
    cell_size: f32,
    storage_width: usize,
    storage_height: usize,
}

impl BoardLayout {
    fn board_width(&self) -> f32 {
        self.cell_size * self.storage_width as f32
    }

    fn board_height(&self) -> f32 {
        self.cell_size * self.storage_height as f32
    }

    fn cell_rect(&self, coord: Coord) -> Rect {
        Rect::new(
            self.x + coord.x as f32 * self.cell_size,
            self.y + coord.y as f32 * self.cell_size,
            self.cell_size,
            self.cell_size,
        )
    }

    fn coord_at(&self, point: Vec2) -> Option<Coord> {
        if point.x < self.x
            || point.y < self.y
            || point.x >= self.x + self.board_width()
            || point.y >= self.y + self.board_height()
        {
            return None;
        }

        Some(Coord::new(
            ((point.x - self.x) / self.cell_size).floor() as i32,
            ((point.y - self.y) / self.cell_size).floor() as i32,
        ))
    }
}

struct Buttons {
    primary: Option<Rect>,
    leave: Option<Rect>,
}

/// One seat of a game: our local game plus, online, the sync layer that
/// mirrors it to the other seat. Clicks apply to `game` exactly like
/// local hotseat play; applied actions are announced and incoming
/// announcements are adopted (see `Replica`).
struct GameView {
    game: Option<Game>,
    /// Our seat online; `None` in local hotseat (anyone may click).
    side: Option<Side>,
    /// Online sync state; `None` in local play.
    net: Option<Online>,
}

struct Online {
    net: NetHandle,
    replica: Replica,
    /// The red seat generates the first game when the other seat fills;
    /// every seat generates its own game on a reset.
    creator: bool,
    /// Native only: the WebSocket URL, shown on the waiting screen.
    #[cfg(not(target_arch = "wasm32"))]
    url: Option<String>,
    /// Whether the room link is up (native reconnects; wasm rooms are
    /// terminal).
    connected: bool,
    opponent_present: bool,
    /// Why the room ended for good, if it did.
    closed: Option<String>,
    /// Wasm only: whether the lobby overlay has been told to hide itself.
    #[cfg(target_arch = "wasm32")]
    view_started: bool,
    /// Wasm only: the peer holding the other seat (directed sends).
    #[cfg(target_arch = "wasm32")]
    peer: Option<String>,
}

impl GameView {
    fn local() -> Self {
        Self {
            game: Some(new_local_game()),
            side: None,
            net: None,
        }
    }

    /// Native online: connect to the relay; our seat arrives with the
    /// hello.
    #[cfg(not(target_arch = "wasm32"))]
    fn relay(url: String) -> Self {
        Self {
            game: None,
            side: None,
            net: Some(Online::relay(url)),
        }
    }

    /// Wasm online: the bridge already put us in a room; the lobby
    /// overlay is still up until the first game state is adopted. The
    /// side comes from the bridge's start event (which the lobby phase
    /// consumes, so it must be handed over here, not re-read later).
    #[cfg(target_arch = "wasm32")]
    fn p2p(side: Side) -> Self {
        Self {
            game: None,
            side: Some(side),
            net: Some(Online::p2p(side)),
        }
    }

    fn pump(&mut self) {
        let Some(online) = &mut self.net else {
            return;
        };
        while let Some(event) = online.net.poll() {
            match event {
                NetEvent::Connected { side } => {
                    self.side = Some(side);
                    online.connected = true;
                    // The room creator takes red — matching the relay
                    // server, which seats the first connection red.
                    online.creator = side == Side::Red;
                }
                #[cfg(not(target_arch = "wasm32"))]
                NetEvent::Disconnected => online.connected = false,
                NetEvent::Closed { reason } => {
                    online.connected = false;
                    if online.closed.is_none() {
                        online.closed = Some(reason);
                    }
                }
                #[cfg(target_arch = "wasm32")]
                NetEvent::Lobby => {
                    // The lobby took over (its overlay shows the reason);
                    // end the game view.
                    online.connected = false;
                    if online.closed.is_none() {
                        online.closed = Some(String::new());
                    }
                }
                #[cfg(target_arch = "wasm32")]
                NetEvent::PeerJoined { peer } => {
                    online.opponent_present = true;
                    if online.peer.is_none() {
                        online.peer = peer;
                    }
                    // Re-seat whoever joined with our current state; the
                    // red seat generates the first game if there is none.
                    if self.game.is_none() && online.creator {
                        self.game = Some(new_local_game());
                    }
                    online.announce(self.game.as_ref());
                }
                #[cfg(not(target_arch = "wasm32"))]
                NetEvent::PeerJoined { .. } => {
                    online.opponent_present = true;
                    if self.game.is_none() && online.creator {
                        self.game = Some(new_local_game());
                    }
                    online.announce(self.game.as_ref());
                }
                #[cfg(target_arch = "wasm32")]
                NetEvent::PeerLeft { peer } => {
                    if online.peer.as_deref() == peer.as_deref() {
                        // The other seat emptied; a later joiner can
                        // reclaim it.
                        online.peer = None;
                        online.opponent_present = false;
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                NetEvent::PeerLeft { .. } => {
                    online.opponent_present = false;
                }
                NetEvent::Sync(sync) => {
                    let current =
                        serde_json::to_value(&self.game).expect("games always serialize");
                    if online.replica.should_adopt(&sync, &current)
                        && let Ok(state) =
                            serde_json::from_value::<Option<Game>>(sync.state.clone())
                    {
                        self.game = state;
                        online.replica.adopt(&sync);
                    }
                }
                // Lobby-scoped events; only the lobby consumes those.
                #[cfg(target_arch = "wasm32")]
                NetEvent::LocalGame => {}
            }
        }

        // The first game state means the game view is live: hand the
        // screen over from the lobby overlay to the canvas.
        #[cfg(target_arch = "wasm32")]
        if !online.view_started && self.game.is_some() {
            online.view_started = true;
            online.net.notify_ready();
        }
    }

    fn can_play(&self) -> bool {
        let Some(game) = &self.game else {
            return false;
        };
        if game.result().is_some() {
            return false;
        }
        match &self.net {
            // Hotseat: whoever's turn it is clicks.
            None => true,
            Some(online) => {
                online.connected
                    && online.opponent_present
                    && self.side == Some(game.current_player())
            }
        }
    }

    fn wants_reset(&self) -> bool {
        match &self.net {
            // Hotseat: the button resets any time.
            None => self.game.is_some(),
            Some(online) => {
                online.connected
                    && online.opponent_present
                    && self.game.as_ref().is_some_and(|game| game.result().is_some())
            }
        }
    }

    /// Leaving only makes sense while the opponent is gone: the game is
    /// paused and abandoned.
    fn wants_leave(&self) -> bool {
        let Some(online) = &self.net else {
            return false;
        };
        online.connected && self.game.is_some() && !online.opponent_present
    }

    /// Applies a board click exactly like hotseat play and announces it
    /// if it changed anything.
    fn click(&mut self, coord: Coord) {
        let Some(game) = &mut self.game else {
            return;
        };
        if handle_local_click(game, coord)
            && let Some(online) = &mut self.net
        {
            online.announce(Some(game));
        }
    }

    /// UX shortcut: with exactly one selectable token there is nothing
    /// to choose, so activate it immediately instead of demanding a
    /// click. Runs after click handling so a deliberate selection click
    /// wins over the automatic one.
    fn auto_advance(&mut self) {
        if !self.can_play() {
            return;
        }
        let Some(game) = &mut self.game else {
            return;
        };
        if game.phase() != Phase::ChooseActivatedToken {
            return;
        }
        let selectable = game.selectable_activated_tokens();
        if let [only] = selectable.as_slice() {
            let id = *only;
            if game.try_select_activated_token(id) && let Some(online) = &mut self.net {
                online.announce(Some(game));
            }
        }
    }

    /// Starts a fresh game and announces it; simultaneous resets
    /// converge deterministically (see `Replica`).
    fn reset(&mut self) {
        self.game = Some(new_local_game());
        if let Some(online) = &mut self.net {
            online.announce(self.game.as_ref());
        }
    }

    /// Abandons the game: wasm returns to the lobby menu (ending the
    /// room); native clears the seat so a later pairing starts fresh.
    fn leave(&mut self) {
        match &mut self.net {
            #[cfg(target_arch = "wasm32")]
            Some(online) => online.net.leave_to_menu(),
            #[cfg(not(target_arch = "wasm32"))]
            Some(online) => {
                self.game = None;
                online.announce(None);
            }
            None => {}
        }
    }
}

impl Online {
    /// Publishes our current state so the other seat can adopt it.
    fn announce(&mut self, game: Option<&Game>) {
        self.replica.version += 1;
        let sync = StateSync {
            version: self.replica.version,
            state: serde_json::to_value(game).expect("games always serialize"),
        };
        #[cfg(target_arch = "wasm32")]
        let peer = self.peer.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let peer: Option<String> = None;
        self.net.announce(&sync, peer.as_deref());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn relay(url: String) -> Self {
        Self {
            net: net::spawn(url.clone()),
            replica: Replica::default(),
            creator: false,
            url: Some(url),
            connected: false,
            opponent_present: false,
            closed: None,
        }
    }

    /// Wasm P2P: the bridge already put us in a room. The room creator
    /// takes red — matching the relay server, which seats the first
    /// connection red.
    #[cfg(target_arch = "wasm32")]
    fn p2p(side: Side) -> Self {
        Self {
            net: NetHandle,
            replica: Replica::default(),
            creator: side == Side::Red,
            connected: true,
            opponent_present: false,
            closed: None,
            view_started: false,
            peer: None,
        }
    }
}

/// The lobby variant exists only on wasm and the mode lives briefly on
/// the stack; the size difference is irrelevant.
#[allow(clippy::large_enum_variant)]
enum Mode {
    /// The game itself: a local hotseat, or one seat of a networked room.
    Game(GameView),
    /// Wasm only: the lobby overlay (HTML) owns the screen; the canvas
    /// idles until the bridge reports the player's choice.
    #[cfg(target_arch = "wasm32")]
    Lobby,
}

impl Mode {
    fn storage_dims(&self) -> (usize, usize) {
        match self {
            Self::Game(view) => view.game.as_ref().map_or(
                (BOARD_WIDTH + 2, BOARD_HEIGHT + 2),
                |game| (game.board().storage_width(), game.board().storage_height()),
            ),
            #[cfg(target_arch = "wasm32")]
            Self::Lobby => (BOARD_WIDTH + 2, BOARD_HEIGHT + 2),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct Args {
    server: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_args() -> Args {
    let mut server = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Hibe di Hopedi");
                println!();
                println!("usage: hibedi-client [--server <ws-url>]");
                println!();
                println!("  --server <url>   play online against another client (ws://host:3000)");
                println!("  (no arguments)   local hotseat game");
                std::process::exit(0);
            }
            "--server" => {
                let value = args.next().unwrap_or_else(|| {
                    eprintln!("--server requires a URL, e.g. --server ws://127.0.0.1:3000");
                    std::process::exit(2);
                });
                server = Some(value);
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

    Args { server }
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

fn new_local_game() -> Game {
    // `macroquad::prelude` re-exports quad_rand as `rand`, so name the real crate.
    Game::new_random(BOARD_WIDTH, BOARD_HEIGHT, &mut ::rand::rng())
}

#[macroquad::main(window_conf)]
async fn main() {
    // The embedded UI fonts must load after macroquad's GPU context is
    // up; everything drawn afterwards goes through them.
    let _ = UI.set(Ui::load());

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Conf sizes are physical window pixels, while the UI is laid out
        // in logical units (macroquad divides by the DPI scale). On hidpi
        // screens, enlarge the window once so the logical workspace matches
        // the ~1152×768 design the layout constants assume.
        let scale = macroquad::miniquad::window::dpi_scale();
        if scale > 1.01 {
            macroquad::miniquad::window::set_window_size(
                (1152.0 * scale) as u32,
                (768.0 * scale) as u32,
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let mut mode = {
        let args = parse_args();
        match args.server {
            Some(raw) => {
                let url = normalize_url(&raw);
                println!("connecting to {url}");
                Mode::Game(GameView::relay(url))
            }
            None => Mode::Game(GameView::local()),
        }
    };
    #[cfg(target_arch = "wasm32")]
    let mut mode = Mode::Lobby;

    // Wakes the JS lobby so it can report the page's room code, if any.
    #[cfg(target_arch = "wasm32")]
    net::bridge_ready();

    loop {
        // Networking first; the lobby may switch modes while pumping.
        #[cfg_attr(not(target_arch = "wasm32"), expect(unused_mut))]
        let mut transition: Option<Mode> = None;
        match &mut mode {
            Mode::Game(view) => {
                view.pump();
                // The room ended (wasm: the host left, or the lobby took
                // over — its overlay shows the reason). Native keeps the
                // view; its waiting panel shows the reason.
                #[cfg(target_arch = "wasm32")]
                if view.net.as_ref().is_some_and(|online| online.closed.is_some()) {
                    transition = Some(Mode::Lobby);
                }
            }
            #[cfg(target_arch = "wasm32")]
            Mode::Lobby => {
                if let Some(next_mode) = lobby_update() {
                    transition = Some(next_mode);
                }
            }
        }

        let layout = layout(&mode);

        // Input, possibly switching modes.
        if transition.is_none() {
            match &mut mode {
                Mode::Game(view) => handle_input(view, &layout),
                #[cfg(target_arch = "wasm32")]
                Mode::Lobby => {}
            }
        }
        if let Some(next) = transition {
            mode = next;
        }

        clear_background(BACKGROUND);
        draw_scene(&mode, &layout);

        next_frame().await;
    }
}

/// Polls the bridge while in the lobby: the HTML overlay owns the menu,
/// and its choice arrives as events.
#[cfg(target_arch = "wasm32")]
fn lobby_update() -> Option<Mode> {
    while let Some(event) = NetHandle.poll() {
        match event {
            NetEvent::Connected { side } => return Some(Mode::Game(GameView::p2p(side))),
            NetEvent::LocalGame => return Some(Mode::Game(GameView::local())),
            NetEvent::Lobby => {}
            _ => {}
        }
    }
    None
}

/// Hotseat input, identical for local and online play: online play only
/// adds the announcement of applied actions (see [`GameView::click`]).
fn handle_input(view: &mut GameView, layout: &Layout) {
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    let mouse: Vec2 = mouse_position().into();

    // Hotseat resets any time; online only once the game is over.
    if is_key_pressed(KeyCode::R) && (view.net.is_none() || view.wants_reset()) {
        view.reset();
    } else if clicked {
        if let Some(rect) = layout.buttons.primary
            && point_in_rect(mouse, rect)
            && view.wants_reset()
        {
            view.reset();
        } else if let Some(rect) = layout.buttons.leave
            && point_in_rect(mouse, rect)
            && view.wants_leave()
        {
            view.leave();
        } else if view.can_play()
            && let Some(coord) = layout.board.coord_at(mouse)
        {
            view.click(coord);
        }
    }

    // A lone selectable token activates without demanding a click.
    view.auto_advance();
}

/// Applies a board click the way a hotseat click works; returns whether
/// anything changed.
fn handle_local_click(game: &mut Game, coord: Coord) -> bool {
    match game.phase() {
        Phase::PlaceInitialToken => game.try_place_initial_token(coord),
        Phase::ChooseJump => {
            // A landing click hops; a click on another glowing token
            // switches the selection (allowed until the first hop).
            game.try_jump_to(coord)
                || game
                    .board()
                    .token_at(coord)
                    .is_some_and(|token| game.try_select_activated_token(token))
        }
        Phase::ChooseActivatedToken => {
            let token = game.board().token_at(coord);
            token.is_some_and(|token| game.try_select_activated_token(token))
        }
        Phase::GameOver => false,
    }
}

/// The whole screen arrangement, computed once per frame from the
/// window size and the current mode. Two candidates are evaluated and
/// the one giving the board more room wins:
///
/// - *side*: the board is centered in the window and the panel sits in
///   the right margin (needs enough margin width),
/// - *stacked*: the board is centered with the panel below it.
struct Layout {
    board: BoardLayout,
    panel: Rect,
    buttons: Buttons,
}

fn layout(mode: &Mode) -> Layout {
    let (storage_width, storage_height) = mode.storage_dims();
    let screen = vec2(screen_width(), screen_height());
    let band_y = HEAD + PAD;
    let band_h = (screen.y - band_y - PAD).max(120.0);
    let avail_w = (screen.x - PAD * 2.0).max(120.0);

    // Side candidate: the panel must fit into the right margin of a
    // board centered in the full window.
    let side_cell = ((avail_w - 2.0 * (PAD + PANEL_W + GAP)) / storage_width as f32)
        .min(band_h / storage_height as f32)
        .max(10.0);
    let side_margin = (screen.x - side_cell * storage_width as f32) / 2.0;
    let side_panel_w = (PANEL_W).min(side_margin - GAP - PAD);

    // Stacked candidate: the panel takes a fixed slice of the band.
    let stack_cell = (avail_w / storage_width as f32)
        .min((band_h - PANEL_H - GAP) / storage_height as f32)
        .max(10.0);

    let side = side_panel_w >= MIN_PANEL_W && side_cell >= stack_cell;
    let cell = if side { side_cell } else { stack_cell };
    let board_size = vec2(cell * storage_width as f32, cell * storage_height as f32);
    // Everything hangs from the top of the content band; no vertical
    // centering.
    let board = BoardLayout {
        x: (screen.x - board_size.x) / 2.0,
        y: band_y,
        cell_size: cell,
        storage_width,
        storage_height,
    };

    let panel = if side {
        Rect::new(board.x + board_size.x + GAP, band_y, side_panel_w, PANEL_H)
    } else {
        let width = avail_w.min(460.0);
        Rect::new(
            (screen.x - width) / 2.0,
            band_y + board_size.y + GAP,
            width,
            PANEL_H,
        )
    };

    // The buttons live in the panel's bottom row; each keeps its slot
    // whether or not it is currently shown (no layout jitter).
    let slot_w = (panel.w - 40.0 - 12.0) / 2.0;
    let slot_y = panel.y + panel.h - 50.0;
    let slot = Rect::new(panel.x + 20.0, slot_y, slot_w, 38.0);
    let buttons = match mode {
        Mode::Game(view) => buttons_for(view, slot, slot_w),
        #[cfg(target_arch = "wasm32")]
        Mode::Lobby => Buttons {
            primary: None,
            leave: None,
        },
    };

    Layout {
        board,
        panel,
        buttons,
    }
}

fn buttons_for(view: &GameView, slot: Rect, slot_w: f32) -> Buttons {
    let leave_slot = Rect::new(slot.x + slot_w + 12.0, slot.y, slot_w, slot.h);
    Buttons {
        primary: view.wants_reset().then_some(slot),
        leave: view.wants_leave().then_some(leave_slot),
    }
}

fn draw_scene(mode: &Mode, layout: &Layout) {
    match mode {
        Mode::Game(view) => {
            draw_header(view);
            if let Some(game) = &view.game {
                // Hints (cyan/gold markers) reveal what the current
                // player can do: always in hotseat, online only while
                // it is actually our turn.
                let show_hints =
                    view.net.is_none() || view.side == Some(game.current_player());
                draw_board(game, &layout.board, show_hints, view.side);
                draw_panel(game, layout, view);
            } else {
                // Wasm: the lobby overlay covers the screen until the
                // first game state arrives; native draws a waiting panel.
                #[cfg(not(target_arch = "wasm32"))]
                draw_waiting(view);
            }
        }
        // Wasm: the lobby overlay owns the screen; draw nothing.
        #[cfg(target_arch = "wasm32")]
        Mode::Lobby => {}
    }
}

/// Title plus, online only, a chip that appears while the link needs
/// attention. A healthy room stays quiet: whose turn it is is visible
/// on the board itself, and who we are is visible on the board and in
/// the score (own cells/score are marked).
fn draw_header(view: &GameView) {
    let width = screen_width();
    let title_size = if width < 460.0 { 20.0 } else { 26.0 };
    bold_text("HIBE DI HOPEDI", PAD + 2.0, 38.0, title_size, WHITE);
    if width >= 700.0 {
        text("A forced-hop territory prototype", PAD + 4.0, 58.0, 14.0, MUTED);
    }

    // Connection chip: only while something is wrong and a game is on
    // screen (the waiting screen has its own messaging).
    let Some(online) = &view.net else {
        return;
    };
    if view.game.is_none() || (online.connected && online.opponent_present) {
        return;
    }
    let label = if online.connected {
        "Opponent disconnected"
    } else {
        "Reconnecting…"
    };
    let chip_w = bold_text_width(label, 15.0) + 24.0;
    let x = width - chip_w - PAD;
    draw_round_rect(x, 16.0, chip_w, 30.0, 10.0, PANEL_EDGE);
    bold_text(label, x + 12.0, 36.0, 15.0, WHITE);
}

fn draw_board(game: &Game, layout: &BoardLayout, show_hints: bool, my_side: Option<Side>) {
    let initial_placements = game.legal_initial_placements();
    let jumps = game.legal_current_jumps();
    let selectable_tokens = game.selectable_activated_tokens();
    let current_token = game.current_token();

    draw_round_rect(
        layout.x - 6.0,
        layout.y - 6.0,
        layout.board_width() + 12.0,
        layout.board_height() + 12.0,
        14.0,
        PANEL,
    );

    for y in 0..game.board().storage_height() as i32 {
        for x in 0..game.board().storage_width() as i32 {
            let coord = Coord::new(x, y);
            let rect = layout.cell_rect(coord);
            let base = match game.board().cell_at(coord).unwrap_or(Cell::Empty) {
                Cell::Empty => EMPTY_CELL,
                Cell::Red => RED_CELL,
                Cell::Green => GREEN_CELL,
            };
            draw_rectangle(rect.x + 1.0, rect.y + 1.0, rect.w - 2.0, rect.h - 2.0, base);

            if game.is_crossed_this_chain(coord) {
                draw_rectangle(
                    rect.x + 1.0,
                    rect.y + 1.0,
                    rect.w - 2.0,
                    rect.h - 2.0,
                    USED_CELL_OVERLAY,
                );
            }

            // Mark the viewer's own territory (online only): a light
            // border around every field of our color.
            if my_side.is_some_and(|my| game.board().cell_at(coord).and_then(Cell::side) == Some(my))
            {
                let inset = (rect.w * 0.05).clamp(2.0, 5.0);
                draw_rectangle_lines(
                    rect.x + inset,
                    rect.y + inset,
                    rect.w - inset * 2.0,
                    rect.h - inset * 2.0,
                    (rect.w * 0.055).clamp(3.0, 6.0),
                    OWN_EDGE,
                );
            }

            if show_hints && initial_placements.contains(&coord) {
                let inset = (rect.w * 0.11).clamp(2.0, 6.0);
                draw_rectangle_lines(
                    rect.x + inset,
                    rect.y + inset,
                    rect.w - inset * 2.0,
                    rect.h - inset * 2.0,
                    (rect.w * 0.08).clamp(3.0, 5.0),
                    CYAN,
                );
                draw_circle(
                    rect.x + rect.w / 2.0,
                    rect.y + rect.h / 2.0,
                    rect.w * 0.10,
                    CYAN,
                );
            }

            if show_hints && jumps.iter().any(|jump| jump.landing == coord) {
                let inset = (rect.w * 0.09).clamp(2.0, 6.0);
                draw_rectangle_lines(
                    rect.x + inset,
                    rect.y + inset,
                    rect.w - inset * 2.0,
                    rect.h - inset * 2.0,
                    (rect.w * 0.10).clamp(3.5, 5.5),
                    GOLD,
                );
                draw_circle(
                    rect.x + rect.w / 2.0,
                    rect.y + rect.h / 2.0,
                    rect.w * 0.13,
                    GOLD,
                );
            }
        }
    }

    for token in game.board().tokens() {
        let rect = layout.cell_rect(token.position);
        let center = vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let radius = rect.w * 0.27;
        let is_current = current_token == Some(token.id);
        let is_selectable = show_hints && selectable_tokens.contains(&token.id);
        let is_pending = game.pending_activated_tokens().contains(&token.id);

        if is_selectable {
            draw_circle(center.x, center.y, radius + 7.0, GOLD);
        } else if is_pending {
            draw_circle_lines(center.x, center.y, radius + 5.0, 2.0, MUTED);
        }

        draw_circle(center.x, center.y, radius, TOKEN_FILL);
        draw_circle_lines(
            center.x,
            center.y,
            radius,
            2.0,
            if is_current { CYAN } else { TOKEN_EDGE },
        );
        draw_circle_lines(
            center.x,
            center.y,
            radius + 1.0,
            1.0,
            if is_current { CYAN } else { TOKEN_EDGE },
        );
    }
}

fn draw_panel(game: &Game, layout: &Layout, view: &GameView) {
    let panel = layout.panel;
    draw_round_rect(panel.x, panel.y, panel.w, panel.h, 16.0, PANEL);
    draw_rectangle_lines(panel.x, panel.y, panel.w, panel.h, 16.0, PANEL_EDGE);

    let x = panel.x + 20.0;
    let width = panel.w - 40.0;

    let (red, green) = game.board().colored_counts();
    draw_count_chip(x, panel.y + 40.0, "Red", red, RED_CELL, view.side == Some(Side::Red));
    draw_count_chip(
        x + width / 2.0,
        panel.y + 40.0,
        "Green",
        green,
        GREEN_CELL,
        view.side == Some(Side::Green),
    );
    draw_line(
        x,
        panel.y + 62.0,
        x + width,
        panel.y + 62.0,
        1.0,
        PANEL_EDGE,
    );

    let status = match &view.net {
        None => status_text(game),
        Some(online) => online_status_text(online, game),
    };
    draw_wrapped_text(&status, x, panel.y + 94.0, width, 24.0, 17.0, WHITE);

    if let Some(rect) = layout.buttons.primary {
        let finished = view.net.is_none() && game.phase() == Phase::GameOver;
        draw_button(
            rect,
            if view.net.is_none() {
                "NEW GAME  R"
            } else {
                "NEW GAME"
            },
            if finished || view.net.is_some() {
                GOLD
            } else {
                PANEL_EDGE
            },
        );
    }
    if let Some(rect) = layout.buttons.leave {
        draw_button(rect, "LEAVE GAME", PANEL_EDGE);
    }
}

/// One score entry: colored dot, label, count. The viewer's own side
/// (online only) gets a backdrop and a ringed dot so ownership reads at
/// a glance.
fn draw_count_chip(x: f32, y: f32, label: &str, count: usize, color: Color, mine: bool) {
    if mine {
        let count_width = bold_text_width(&count.to_string(), 19.0);
        let w = 76.0 + count_width + 14.0;
        draw_round_rect(x - 10.0, y - 20.0, w, 40.0, 10.0, PANEL_EDGE);
    }
    draw_circle(x + 6.0, y - 5.0, 6.0, color);
    if mine {
        draw_circle_lines(x + 6.0, y - 5.0, 9.0, 2.0, OWN_EDGE);
    }
    bold_text(label, x + 18.0, y, 16.0, WHITE);
    bold_text(&count.to_string(), x + 76.0, y, 19.0, WHITE);
}

fn draw_button(rect: Rect, label: &str, fill: Color) {
    draw_round_rect(rect.x, rect.y, rect.w, rect.h, 9.0, fill);
    let text_width = bold_text_width(label, 15.0);
    bold_text(
        label,
        rect.x + (rect.w - text_width) / 2.0,
        rect.y + rect.h / 2.0 + 5.5,
        15.0,
        WHITE,
    );
}

/// The native waiting screen (the wasm lobby overlay covers this case).
#[cfg(not(target_arch = "wasm32"))]
fn draw_waiting(view: &GameView) {
    let width = (screen_width() - 48.0).min(560.0);
    let height = 210.0;
    let panel = Rect::new(
        (screen_width() - width) / 2.0,
        ((screen_height() - height) / 2.0 - 10.0).max(90.0),
        width,
        height,
    );
    draw_round_rect(panel.x, panel.y, panel.w, panel.h, 18.0, PANEL);
    draw_rectangle_lines(panel.x, panel.y, panel.w, panel.h, 18.0, PANEL_EDGE);

    let online = view.net.as_ref().expect("the waiting screen is online-only");
    let (title, body) = if let Some(reason) = &online.closed {
        ("Connection ended".to_owned(), reason.clone())
    } else if !online.connected {
        (
            "Connection lost".to_owned(),
            format!(
                "Reconnecting to {}…",
                online.url.as_deref().unwrap_or("the server")
            ),
        )
    } else {
        (
            "Waiting for a second player".to_owned(),
            format!(
                "Both clients must connect to {}",
                online.url.as_deref().unwrap_or("the server")
            ),
        )
    };

    let title_width = bold_text_width(&title, 24.0);
    bold_text(
        &title,
        panel.x + (panel.w - title_width) / 2.0,
        panel.y + 74.0,
        24.0,
        WHITE,
    );
    draw_wrapped_text(&body, panel.x + 34.0, panel.y + 116.0, panel.w - 68.0, 22.0, 16.0, MUTED);
    draw_wrapped_text(
        "Seats are assigned automatically: first red, second green.",
        panel.x + 34.0,
        panel.y + 164.0,
        panel.w - 68.0,
        20.0,
        15.0,
        MUTED,
    );
}

fn online_status_text(online: &Online, game: &Game) -> String {
    if !online.connected {
        return "Connection lost. Trying to reconnect…".to_owned();
    }
    if game.result().is_none() && !online.opponent_present {
        return "Waiting for your opponent to reconnect. You can leave the game.".to_owned();
    }
    status_text(game)
}

fn status_text(game: &Game) -> String {
    match game.phase() {
        Phase::PlaceInitialToken => {
            format!(
                "{}: place a token on any cyan-marked cell.",
                game.current_player().label()
            )
        }
        Phase::ChooseJump => {
            "Keep hopping — every available jump is compulsory.".to_owned()
        }
        Phase::ChooseActivatedToken => {
            let options = game.selectable_activated_tokens().len();
            format!(
                "Choose one of the {options} glowing token{} to jump with.",
                if options == 1 { "" } else { "s" }
            )
        }
        Phase::GameOver => match game.result() {
            Some(GameResult::Winner {
                side,
                reason: WinReason::Monochrome,
            }) => format!(
                "{} wins: every colored field is {}.",
                side.label(),
                side.label().to_lowercase()
            ),
            Some(GameResult::Winner {
                side,
                reason: WinReason::Majority,
            }) => format!("{} wins the final colored-cell majority.", side.label()),
            Some(GameResult::Draw) => {
                "Draw: red and green own the same number of colored cells.".to_owned()
            }
            None => "Game complete.".to_owned(),
        },
    }
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Hibe di Hopedi".to_owned(),
        // Physical window pixels; main() enlarges the window on hidpi
        // screens so the logical workspace matches the ~1152×768 design
        // the layout constants assume (identical to the web build).
        window_width: 1152,
        window_height: 768,
        window_resizable: true,
        // Size the drawing buffer in physical pixels on hidpi screens;
        // macroquad keeps screen coordinates and text layout logical, and
        // rasterizes glyphs at the higher resolution (crisp text).
        high_dpi: true,
        ..Default::default()
    }
}

fn draw_wrapped_text(
    body: &str,
    x: f32,
    mut y: f32,
    max_width: f32,
    line_height: f32,
    size: f32,
    color: Color,
) {
    let mut line = String::new();
    for word in body.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };

        if !line.is_empty() && text_width(&candidate, size) > max_width {
            text(&line, x, y, size, color);
            y += line_height;
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }

    if !line.is_empty() {
        text(&line, x, y, size, color);
    }
}

fn draw_round_rect(x: f32, y: f32, width: f32, height: f32, radius: f32, color: Color) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    draw_rectangle(x + radius, y, width - radius * 2.0, height, color);
    draw_rectangle(x, y + radius, width, height - radius * 2.0, color);
    draw_circle(x + radius, y + radius, radius, color);
    draw_circle(x + width - radius, y + radius, radius, color);
    draw_circle(x + radius, y + height - radius, radius, color);
    draw_circle(x + width - radius, y + height - radius, radius, color);
}

fn point_in_rect(point: Vec2, rect: Rect) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.w
        && point.y >= rect.y
        && point.y <= rect.y + rect.h
}
