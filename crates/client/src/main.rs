mod net;

use hibedi_core::{
    BOARD_HEIGHT, BOARD_WIDTH, Cell, ClientMessage, Coord, Game, GameResult, Phase, ServerMessage,
    Side, Status, WinReason,
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
const MUTED: Color = Color::from_rgba(169, 178, 191, 255);

struct BoardLayout {
    x: f32,
    y: f32,
    cell_size: f32,
    storage_width: usize,
    storage_height: usize,
    compact: bool,
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

#[derive(Default)]
struct Buttons {
    primary: Option<Rect>,
    leave: Option<Rect>,
}

struct Online {
    url: String,
    net: NetHandle,
    side: Option<Side>,
    game: Option<Game>,
    opponent_present: bool,
    status: Status,
    connected: bool,
    toast: Option<(String, f64)>,
}

impl Online {
    fn connect(url: String) -> Self {
        let net = net::spawn(url.clone());

        Self {
            url,
            net,
            side: None,
            game: None,
            opponent_present: false,
            status: Status::Waiting,
            connected: false,
            toast: None,
        }
    }

    fn pump(&mut self) {
        while let Ok(event) = self.net.events.try_recv() {
            match event {
                NetEvent::Connected => self.connected = true,
                NetEvent::Disconnected => self.connected = false,
                NetEvent::Message(message) => self.apply(message),
            }
        }

        if let Some((_, expires)) = &self.toast
            && get_time() > *expires
        {
            self.toast = None;
        }
    }

    fn apply(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome { side } => {
                self.side = Some(side);
            }
            ServerMessage::State {
                game,
                side,
                opponent_present,
                status,
            } => {
                self.side = Some(side);
                self.game = game;
                self.opponent_present = opponent_present;
                self.status = status;
            }
            ServerMessage::Error { reason } => {
                self.toast = Some((reason, get_time() + 4.0));
            }
        }
    }

    fn can_play(&self) -> bool {
        self.connected
            && self.status == Status::Playing
            && self.opponent_present
            && self
                .game
                .as_ref()
                .is_some_and(|game| self.side == Some(game.current_player()))
    }

    fn wants_new_game(&self) -> bool {
        self.connected
            && self.status == Status::Finished
            && self.opponent_present
            && self.game.is_some()
    }

    fn wants_leave(&self) -> bool {
        self.connected && self.status == Status::Paused
    }

    fn handle_click(&mut self, layout: &BoardLayout, buttons: &Buttons, mouse: Vec2) {
        if let Some(rect) = buttons.primary
            && point_in_rect(mouse, rect)
            && self.wants_new_game()
        {
            self.net.send(ClientMessage::NewGame);
            return;
        }
        if let Some(rect) = buttons.leave
            && point_in_rect(mouse, rect)
            && self.wants_leave()
        {
            self.net.send(ClientMessage::Leave);
            return;
        }

        if !self.can_play() {
            return;
        }
        let (Some(game), Some(coord)) = (&self.game, layout.coord_at(mouse)) else {
            return;
        };
        if let Some(action) = client_action(game, coord) {
            self.net.send(action);
        }
    }

    fn draw_toast(&self) {
        let Some((text, expires)) = &self.toast else {
            return;
        };
        if get_time() > *expires {
            return;
        }

        let width = (screen_width() - 60.0).min(520.0);
        let x = (screen_width() - width) / 2.0;
        let y = screen_height() - 104.0;
        draw_round_rect(x, y, width, 66.0, 12.0, PANEL);
        draw_rectangle_lines(x, y, width, 66.0, 12.0, GOLD);
        draw_wrapped_text(text, x + 18.0, y + 28.0, width - 36.0, 19.0, 15.0, WHITE);
    }
}

enum Mode {
    Local { game: Game },
    Online(Online),
}

impl Mode {
    fn storage_dims(&self) -> (usize, usize) {
        match self {
            Self::Local { game } => (game.board().storage_width(), game.board().storage_height()),
            Self::Online(online) => online
                .game
                .as_ref()
                .map_or((BOARD_WIDTH + 2, BOARD_HEIGHT + 2), |game| {
                    (game.board().storage_width(), game.board().storage_height())
                }),
        }
    }
}

struct Args {
    server: Option<String>,
}

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
    let args = parse_args();
    let mut mode = match args.server {
        Some(raw) => {
            let url = normalize_url(&raw);
            println!("connecting to {url}");
            Mode::Online(Online::connect(url))
        }
        None => Mode::Local {
            game: new_local_game(),
        },
    };

    loop {
        if let Mode::Online(online) = &mut mode {
            online.pump();
        }

        let (storage_width, storage_height) = mode.storage_dims();
        let layout = board_layout(storage_width, storage_height);
        let buttons = buttons_for(&mode, &layout);

        handle_input(&mut mode, &layout, &buttons);

        clear_background(BACKGROUND);
        draw_scene(&mode, &layout, &buttons);

        next_frame().await;
    }
}

fn handle_input(mode: &mut Mode, layout: &BoardLayout, buttons: &Buttons) {
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    let mouse: Vec2 = mouse_position().into();

    match mode {
        Mode::Local { game } => {
            if is_key_pressed(KeyCode::R)
                || (clicked
                    && buttons
                        .primary
                        .is_some_and(|rect| point_in_rect(mouse, rect)))
            {
                *game = new_local_game();
            } else if clicked && let Some(coord) = layout.coord_at(mouse) {
                handle_local_click(game, coord);
            }
        }
        Mode::Online(online) => {
            if is_key_pressed(KeyCode::R) && online.wants_new_game() {
                online.net.send(ClientMessage::NewGame);
            } else if clicked {
                online.handle_click(layout, buttons, mouse);
            }
        }
    }
}

fn handle_local_click(game: &mut Game, coord: Coord) {
    match game.phase() {
        Phase::PlaceInitialToken => {
            game.try_place_initial_token(coord);
        }
        Phase::ChooseJump => {
            if let Some(jump) = game
                .legal_current_jumps()
                .into_iter()
                .find(|jump| jump.landing == coord)
            {
                game.try_jump(jump.direction);
            }
        }
        Phase::ChooseActivatedToken => {
            if let Some(token) = game.board().token_at(coord) {
                game.try_select_activated_token(token);
            }
        }
        Phase::GameOver => {}
    }
}

fn client_action(game: &Game, coord: Coord) -> Option<ClientMessage> {
    match game.phase() {
        Phase::PlaceInitialToken => Some(ClientMessage::Place {
            x: coord.x,
            y: coord.y,
        }),
        Phase::ChooseJump => game
            .legal_current_jumps()
            .into_iter()
            .find(|jump| jump.landing == coord)
            .map(|jump| ClientMessage::Jump {
                direction: jump.direction,
            }),
        Phase::ChooseActivatedToken => game
            .board()
            .token_at(coord)
            .map(|id| ClientMessage::SelectToken { id }),
        Phase::GameOver => None,
    }
}

fn board_layout(storage_width: usize, storage_height: usize) -> BoardLayout {
    let compact = screen_width() < 820.0 || screen_width() / screen_height() < 1.15;
    let horizontal_padding = 26.0;
    let top = 70.0;
    let bottom = if compact { 180.0 } else { 36.0 };
    let side_reserve = if compact { 0.0 } else { 292.0 };
    let available_width = (screen_width() - horizontal_padding * 2.0 - side_reserve).max(120.0);
    let available_height = (screen_height() - top - bottom).max(120.0);
    let cell_size = (available_width / storage_width as f32)
        .min(available_height / storage_height as f32)
        .max(10.0);
    let board_width = cell_size * storage_width as f32;

    BoardLayout {
        x: if compact {
            (screen_width() - board_width) / 2.0
        } else {
            horizontal_padding
        },
        y: top + (available_height - cell_size * storage_height as f32) / 2.0,
        cell_size,
        storage_width,
        storage_height,
        compact,
    }
}

fn buttons_for(mode: &Mode, layout: &BoardLayout) -> Buttons {
    match mode {
        Mode::Local { .. } => Buttons {
            primary: Some(desktop_or_compact_rect(layout, 438.0)),
            leave: None,
        },
        Mode::Online(online) => Buttons {
            primary: online.wants_new_game().then(|| {
                if layout.compact {
                    compact_button_rect(layout, 0.0)
                } else {
                    desktop_or_compact_rect(layout, 420.0)
                }
            }),
            leave: online
                .wants_leave()
                .then(|| leave_rect(layout, online.wants_new_game())),
        },
    }
}

fn desktop_or_compact_rect(layout: &BoardLayout, desktop_y: f32) -> Rect {
    if layout.compact {
        compact_button_rect(layout, 0.0)
    } else {
        Rect::new(
            layout.x + layout.board_width() + 28.0,
            desktop_y,
            190.0,
            36.0,
        )
    }
}

fn compact_button_rect(layout: &BoardLayout, offset: f32) -> Rect {
    Rect::new(
        screen_width() - 130.0,
        layout.y + layout.board_height() + 89.0 + offset,
        108.0,
        34.0,
    )
}

fn leave_rect(layout: &BoardLayout, primary_visible: bool) -> Rect {
    if layout.compact {
        compact_button_rect(layout, if primary_visible { 39.0 } else { 0.0 })
    } else {
        Rect::new(layout.x + layout.board_width() + 28.0, 464.0, 190.0, 36.0)
    }
}

fn draw_scene(mode: &Mode, layout: &BoardLayout, buttons: &Buttons) {
    match mode {
        Mode::Local { game } => {
            draw_header(Some(game), None, true, layout);
            draw_board(game, layout);
            draw_panel(game, layout, buttons, None);
        }
        Mode::Online(online) => {
            draw_header(online.game.as_ref(), online.side, online.connected, layout);
            match &online.game {
                Some(game) => {
                    draw_board(game, layout);
                    draw_panel(game, layout, buttons, Some(online));
                }
                None => draw_waiting(online, layout),
            }
            online.draw_toast();
        }
    }
}

fn draw_header(game: Option<&Game>, my_side: Option<Side>, connected: bool, layout: &BoardLayout) {
    let narrow = screen_width() < 430.0;
    let title = if narrow { "HOPEDI" } else { "HIBE DI HOPEDI" };
    let title_size = if layout.compact { 22.0 } else { 34.0 };
    draw_text(title, 26.0, 37.0, title_size, WHITE);
    if !layout.compact {
        draw_text("A forced-hop territory prototype", 28.0, 57.0, 15.0, MUTED);
    }

    let (turn, turn_color) = header_chip(game, my_side, connected);
    let width = measure_text(&turn, None, 18, 1.0).width + 24.0;
    let x = screen_width() - width - 26.0;
    draw_round_rect(x, 18.0, width, 31.0, 10.0, turn_color);
    draw_text(&turn, x + 12.0, 39.5, 18.0, WHITE);
}

fn header_chip(game: Option<&Game>, my_side: Option<Side>, connected: bool) -> (String, Color) {
    if !connected {
        return ("Reconnecting…".to_owned(), PANEL_EDGE);
    }

    let Some(game) = game else {
        return ("Waiting…".to_owned(), GOLD);
    };

    if let Some(result) = game.result() {
        return match result {
            GameResult::Winner { side, .. } => (format!("{} wins", side.label()), side_color(side)),
            GameResult::Draw => ("Draw".to_owned(), PANEL_EDGE),
        };
    }

    let current = game.current_player();
    let label = match my_side {
        Some(me) if me == current => format!("Your turn ({})", current.label()),
        Some(_) => format!("Opponent ({})", current.label()),
        None => format!("{} to move", current.label()),
    };
    (label, side_color(current))
}

fn draw_board(game: &Game, layout: &BoardLayout) {
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

            if initial_placements.contains(&coord) {
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

            if jumps.iter().any(|jump| jump.landing == coord) {
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
        let is_selectable = selectable_tokens.contains(&token.id);
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

fn draw_panel(game: &Game, layout: &BoardLayout, buttons: &Buttons, online: Option<&Online>) {
    let (x, y, width) = if layout.compact {
        (
            22.0,
            layout.y + layout.board_height() + 22.0,
            screen_width() - 44.0,
        )
    } else {
        (
            layout.x + layout.board_width() + 28.0,
            86.0,
            screen_width() - (layout.x + layout.board_width() + 54.0),
        )
    };
    let height = if layout.compact {
        146.0
    } else if online.is_some() {
        448.0
    } else {
        396.0
    };

    draw_round_rect(x, y, width, height, 16.0, PANEL);
    draw_rectangle_lines(x, y, width, height, 16.0, PANEL_EDGE);

    let (red, green) = game.board().colored_counts();
    draw_text("BOARD STATE", x + 18.0, y + 29.0, 16.0, MUTED);
    draw_count_row(x + 18.0, y + 58.0, "RED", red, RED_CELL);
    draw_count_row(x + 18.0, y + 84.0, "GREEN", green, GREEN_CELL);

    let status = online.map_or_else(
        || status_text(game),
        |online| online_status_text(online, game),
    );

    if !layout.compact {
        draw_line(
            x + 18.0,
            y + 106.0,
            x + width - 18.0,
            y + 106.0,
            1.0,
            PANEL_EDGE,
        );
        draw_text("CURRENT ACTION", x + 18.0, y + 135.0, 16.0, MUTED);
        draw_wrapped_text(
            &status,
            x + 18.0,
            y + 161.0,
            width - 36.0,
            20.0,
            18.0,
            WHITE,
        );
        draw_text(
            "Every available jump is compulsory.",
            x + 18.0,
            y + 266.0,
            15.0,
            MUTED,
        );
        draw_text(
            "Gold: legal landing / active token",
            x + 18.0,
            y + 291.0,
            15.0,
            MUTED,
        );
        draw_text(
            "Cyan: legal placement / moving token",
            x + 18.0,
            y + 314.0,
            15.0,
            MUTED,
        );

        if let Some(online) = online {
            draw_online_lines(online, x + 18.0, y + 344.0);
        }
    } else {
        draw_wrapped_text(
            &status,
            x + 18.0,
            y + 113.0,
            width - 148.0,
            18.0,
            15.0,
            WHITE,
        );
    }

    if let Some(rect) = buttons.primary {
        let finished = online.is_none() && game.phase() == Phase::GameOver;
        draw_button(
            rect,
            if layout.compact || online.is_some() {
                "NEW GAME"
            } else {
                "NEW GAME  R"
            },
            if finished || online.is_some() {
                GOLD
            } else {
                PANEL_EDGE
            },
        );
    }

    if let Some(rect) = buttons.leave {
        draw_button(rect, "LEAVE GAME", PANEL_EDGE);
    }
}

fn draw_online_lines(online: &Online, x: f32, y: f32) {
    let you = match online.side {
        Some(side) => format!("You are {}", side.label()),
        None => "Not seated yet".to_owned(),
    };
    draw_circle(x + 6.0, y - 5.0, 6.0, online.side.map_or(MUTED, side_color));
    draw_text(&you, x + 20.0, y, 15.0, WHITE);

    let connection = if !online.connected {
        "Reconnecting…"
    } else if online.opponent_present {
        "Opponent connected"
    } else {
        "Opponent disconnected"
    };
    draw_text(connection, x + 20.0, y + 24.0, 15.0, MUTED);
}

fn draw_button(rect: Rect, label: &str, fill: Color) {
    draw_round_rect(rect.x, rect.y, rect.w, rect.h, 9.0, fill);
    let text = measure_text(label, None, 15, 1.0);
    draw_text(
        label,
        rect.x + (rect.w - text.width) / 2.0,
        rect.y + 24.0,
        15.0,
        WHITE,
    );
}

fn draw_waiting(online: &Online, layout: &BoardLayout) {
    let width = if layout.compact {
        screen_width() - 48.0
    } else {
        560.0
    };
    let height = 210.0;
    let x = (screen_width() - width) / 2.0;
    let y = ((screen_height() - height) / 2.0 - 10.0).max(90.0);

    draw_round_rect(x, y, width, height, 18.0, PANEL);
    draw_rectangle_lines(x, y, width, height, 18.0, PANEL_EDGE);

    let (title, body) = if !online.connected {
        (
            "Connection lost",
            format!("Reconnecting to {}…", online.url),
        )
    } else {
        (
            "Waiting for a second player",
            format!("Both clients must connect to {}", online.url),
        )
    };

    let title_width = measure_text(title, None, 24, 1.0).width;
    draw_text(
        title,
        x + (width - title_width) / 2.0,
        y + 74.0,
        24.0,
        WHITE,
    );
    draw_wrapped_text(&body, x + 34.0, y + 116.0, width - 68.0, 22.0, 16.0, MUTED);
    draw_wrapped_text(
        "Seats are assigned automatically: first red, second green.",
        x + 34.0,
        y + 164.0,
        width - 68.0,
        20.0,
        15.0,
        MUTED,
    );
}

fn online_status_text(online: &Online, game: &Game) -> String {
    if !online.connected {
        return "Connection lost. Trying to reconnect…".to_owned();
    }
    if online.status == Status::Paused {
        return "Waiting for your opponent to reconnect. You can leave the game.".to_owned();
    }
    status_text(game)
}

fn draw_count_row(x: f32, y: f32, label: &str, count: usize, color: Color) {
    draw_circle(x + 6.0, y - 5.0, 6.0, color);
    draw_text(label, x + 20.0, y, 16.0, WHITE);
    let count = count.to_string();
    draw_text(&count, x + 104.0, y, 19.0, WHITE);
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
            "Choose a gold landing cell. Continue until this token is blocked.".to_owned()
        }
        Phase::ChooseActivatedToken => {
            let options = game.selectable_activated_tokens().len();
            format!(
                "Choose one of {options} activated token{} with a legal jump.",
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
        window_width: 1120,
        window_height: 760,
        window_resizable: true,
        ..Default::default()
    }
}

fn draw_wrapped_text(
    text: &str,
    x: f32,
    mut y: f32,
    max_width: f32,
    line_height: f32,
    size: f32,
    color: Color,
) {
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };

        if !line.is_empty() && measure_text(&candidate, None, size as u16, 1.0).width > max_width {
            draw_text(&line, x, y, size, color);
            y += line_height;
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }

    if !line.is_empty() {
        draw_text(&line, x, y, size, color);
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

fn side_color(side: Side) -> Color {
    match side {
        Side::Red => RED_CELL,
        Side::Green => GREEN_CELL,
    }
}
