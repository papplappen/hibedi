mod game;

use game::{Cell, Coord, Game, GameResult, Phase, Side, WinReason};
use macroquad::prelude::*;

const BOARD_WIDTH: usize = 4;
const BOARD_HEIGHT: usize = 5;

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
    compact: bool,
}

impl BoardLayout {
    fn board_width(&self, game: &Game) -> f32 {
        self.cell_size * game.board().storage_width() as f32
    }

    fn board_height(&self, game: &Game) -> f32 {
        self.cell_size * game.board().storage_height() as f32
    }

    fn cell_rect(&self, coord: Coord) -> Rect {
        Rect::new(
            self.x + coord.x as f32 * self.cell_size,
            self.y + coord.y as f32 * self.cell_size,
            self.cell_size,
            self.cell_size,
        )
    }

    fn coord_at(&self, game: &Game, point: Vec2) -> Option<Coord> {
        if point.x < self.x
            || point.y < self.y
            || point.x >= self.x + self.board_width(game)
            || point.y >= self.y + self.board_height(game)
        {
            return None;
        }

        Some(Coord::new(
            ((point.x - self.x) / self.cell_size).floor() as i32,
            ((point.y - self.y) / self.cell_size).floor() as i32,
        ))
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

#[macroquad::main(window_conf)]
async fn main() {
    let mut game = Game::new_random(BOARD_WIDTH, BOARD_HEIGHT);

    loop {
        let layout = board_layout(&game);
        let reset_rect = reset_button_rect(&layout, &game);

        if is_key_pressed(KeyCode::R)
            || (is_mouse_button_pressed(MouseButton::Left)
                && point_in_rect(mouse_position().into(), reset_rect))
        {
            game = Game::new_random(BOARD_WIDTH, BOARD_HEIGHT);
        } else if is_mouse_button_pressed(MouseButton::Left) {
            let mouse: Vec2 = mouse_position().into();
            if let Some(coord) = layout.coord_at(&game, mouse) {
                handle_board_click(&mut game, coord);
            }
        }

        clear_background(BACKGROUND);
        draw_header(&game, &layout);
        draw_board(&game, &layout);
        draw_panel(&game, &layout, reset_rect);

        next_frame().await;
    }
}

fn board_layout(game: &Game) -> BoardLayout {
    let compact = screen_width() < 820.0 || screen_width() / screen_height() < 1.15;
    let horizontal_padding = 26.0;
    let top = 70.0;
    let bottom = if compact { 180.0 } else { 36.0 };
    let side_reserve = if compact { 0.0 } else { 292.0 };
    let available_width = (screen_width() - horizontal_padding * 2.0 - side_reserve).max(120.0);
    let available_height = (screen_height() - top - bottom).max(120.0);
    let cell_size = (available_width / game.board().storage_width() as f32)
        .min(available_height / game.board().storage_height() as f32)
        .max(10.0);
    let board_width = cell_size * game.board().storage_width() as f32;

    BoardLayout {
        x: if compact {
            (screen_width() - board_width) / 2.0
        } else {
            horizontal_padding
        },
        y: top + (available_height - cell_size * game.board().storage_height() as f32) / 2.0,
        cell_size,
        compact,
    }
}

fn handle_board_click(game: &mut Game, coord: Coord) {
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

fn draw_header(game: &Game, layout: &BoardLayout) {
    let narrow = screen_width() < 430.0;
    let title = if narrow { "HOPEDI" } else { "HIBE DI HOPEDI" };
    let title_size = if layout.compact { 22.0 } else { 34.0 };
    draw_text(title, 26.0, 37.0, title_size, WHITE);
    if !layout.compact {
        draw_text("A forced-hop territory prototype", 28.0, 57.0, 15.0, MUTED);
    }

    let (turn, turn_color) = match game.result() {
        Some(GameResult::Winner { side, .. }) => {
            (format!("{} wins", side.label()), side_color(side))
        }
        Some(GameResult::Draw) => ("Draw".to_owned(), PANEL_EDGE),
        None => (
            format!("{} to move", game.current_player().label()),
            side_color(game.current_player()),
        ),
    };
    let width = measure_text(&turn, None, 18, 1.0).width + 24.0;
    let x = screen_width() - width - 26.0;
    draw_round_rect(x, 18.0, width, 31.0, 10.0, turn_color);
    draw_text(&turn, x + 12.0, 39.5, 18.0, WHITE);
}

fn draw_board(game: &Game, layout: &BoardLayout) {
    let initial_placements = game.legal_initial_placements();
    let jumps = game.legal_current_jumps();
    let selectable_tokens = game.selectable_activated_tokens();
    let current_token = game.current_token();

    draw_round_rect(
        layout.x - 6.0,
        layout.y - 6.0,
        layout.board_width(game) + 12.0,
        layout.board_height(game) + 12.0,
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

fn draw_panel(game: &Game, layout: &BoardLayout, reset_rect: Rect) {
    let (x, y, width) = if layout.compact {
        (
            22.0,
            layout.y + layout.board_height(game) + 22.0,
            screen_width() - 44.0,
        )
    } else {
        (
            layout.x + layout.board_width(game) + 28.0,
            86.0,
            screen_width() - (layout.x + layout.board_width(game) + 54.0),
        )
    };
    let height = if layout.compact { 146.0 } else { 396.0 };

    draw_round_rect(x, y, width, height, 16.0, PANEL);
    draw_rectangle_lines(x, y, width, height, 16.0, PANEL_EDGE);

    let (red, green) = game.board().colored_counts();
    draw_text("BOARD STATE", x + 18.0, y + 29.0, 16.0, MUTED);
    draw_count_row(x + 18.0, y + 58.0, "RED", red, RED_CELL);
    draw_count_row(x + 18.0, y + 84.0, "GREEN", green, GREEN_CELL);

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
            &status_text(game),
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
    } else {
        draw_wrapped_text(
            &status_text(game),
            x + 18.0,
            y + 113.0,
            width - 148.0,
            18.0,
            15.0,
            WHITE,
        );
    }

    draw_round_rect(
        reset_rect.x,
        reset_rect.y,
        reset_rect.w,
        reset_rect.h,
        9.0,
        if game.phase() == Phase::GameOver {
            GOLD
        } else {
            PANEL_EDGE
        },
    );
    let label = "NEW GAME  R";
    let text = measure_text(label, None, 15, 1.0);
    draw_text(
        label,
        reset_rect.x + (reset_rect.w - text.width) / 2.0,
        reset_rect.y + 24.0,
        15.0,
        WHITE,
    );
}

fn reset_button_rect(layout: &BoardLayout, game: &Game) -> Rect {
    if layout.compact {
        Rect::new(
            screen_width() - 130.0,
            layout.y + layout.board_height(game) + 89.0,
            108.0,
            34.0,
        )
    } else {
        Rect::new(
            layout.x + layout.board_width(game) + 28.0,
            438.0,
            190.0,
            36.0,
        )
    }
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
