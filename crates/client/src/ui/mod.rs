//! Drawing toolkit and screen composition. Placement is derived, not
//! tuned: rectangles are split with [`RectExt`], single lines of text are
//! centered in their boxes with [`label`], and every height a layout
//! depends on is a sum of named constants.

mod board;
mod hud;
pub mod layout;

use std::sync::OnceLock;

use hibedi_core::Side;
use macroquad::prelude::*;

pub use board::BoardLayout;
use hud::Presence;
use layout::{Layout, PlayArea};

use crate::play::Match;

pub mod palette {
    use macroquad::color::Color;

    pub const BACKGROUND: Color = Color::from_rgba(17, 21, 30, 255);
    /// Card surfaces.
    pub const PANEL: Color = Color::from_rgba(31, 38, 51, 255);
    /// Card borders, neutral buttons, highlighted tiles.
    pub const RAISED: Color = Color::from_rgba(67, 77, 96, 255);
    pub const TEXT: Color = Color::from_rgba(255, 255, 255, 255);
    pub const MUTED: Color = Color::from_rgba(169, 178, 191, 255);

    pub const EMPTY_CELL: Color = Color::from_rgba(228, 224, 210, 255);
    pub const RED_CELL: Color = Color::from_rgba(211, 79, 76, 255);
    pub const GREEN_CELL: Color = Color::from_rgba(53, 158, 108, 255);
    pub const USED_CELL_OVERLAY: Color = Color::from_rgba(43, 48, 57, 185);
    pub const TOKEN_FILL: Color = Color::from_rgba(24, 31, 43, 255);
    pub const TOKEN_EDGE: Color = Color::from_rgba(239, 236, 220, 255);
    /// Jump landings, glowing tokens, primary buttons.
    pub const GOLD: Color = Color::from_rgba(194, 137, 38, 255);
    /// Placement cells and the token currently jumping.
    pub const CYAN: Color = Color::from_rgba(33, 139, 154, 255);
    /// Marks what belongs to the viewer online.
    pub const OWN_EDGE: Color = Color::from_rgba(245, 242, 232, 255);
}

pub fn side_color(side: Side) -> Color {
    match side {
        Side::Red => palette::RED_CELL,
        Side::Green => palette::GREEN_CELL,
    }
}

// --- text ---

/// Embedded UI fonts (SIL OFL, see `assets/OFL.txt`). Atkinson
/// Hyperlegible is a legibility-first typeface; unlike the built-in font
/// it rasterizes anti-aliased at every size.
struct Fonts {
    regular: Font,
    bold: Font,
}

static FONTS: OnceLock<Fonts> = OnceLock::new();

/// Loads the UI fonts; must run once macroquad's GPU context is up.
pub fn load_fonts() {
    let load = |bytes: &[u8]| load_ttf_font_from_bytes(bytes).expect("embedded fonts parse");
    let _ = FONTS.set(Fonts {
        regular: load(include_bytes!(
            "../../assets/AtkinsonHyperlegible-Regular.ttf"
        )),
        bold: load(include_bytes!("../../assets/AtkinsonHyperlegible-Bold.ttf")),
    });
}

#[derive(Clone, Copy)]
pub struct TextStyle {
    pub bold: bool,
    pub size: u16,
    pub color: Color,
}

impl TextStyle {
    pub const fn regular(size: u16, color: Color) -> Self {
        Self {
            bold: false,
            size,
            color,
        }
    }

    pub const fn bold(size: u16, color: Color) -> Self {
        Self {
            bold: true,
            size,
            color,
        }
    }

    pub const fn with_color(self, color: Color) -> Self {
        Self { color, ..self }
    }

    fn font(&self) -> &'static Font {
        let fonts = FONTS.get().expect("fonts are loaded at startup");
        if self.bold {
            &fonts.bold
        } else {
            &fonts.regular
        }
    }

    pub fn width(&self, text: &str) -> f32 {
        measure_text(text, Some(self.font()), self.size, 1.0).width
    }

    /// Height of the capital letters; lines are centered on it, so they
    /// sit visually centered whatever letters they contain.
    fn cap_height(&self) -> f32 {
        measure_text("H", Some(self.font()), self.size, 1.0).height
    }

    fn draw(&self, text: &str, x: f32, baseline: f32) {
        draw_text_ex(
            text,
            x,
            baseline,
            TextParams {
                font: Some(self.font()),
                font_size: self.size,
                color: self.color,
                ..Default::default()
            },
        );
    }
}

#[derive(Clone, Copy)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Draws one line of text vertically centered in `rect`.
pub fn label(rect: Rect, text: &str, style: TextStyle, align: Align) {
    let width = style.width(text);
    let x = match align {
        Align::Left => rect.x,
        Align::Center => rect.x + (rect.w - width) / 2.0,
        Align::Right => rect.right() - width,
    };
    style.draw(text, x, rect.center().y + style.cap_height() / 2.0);
}

/// Word-wraps `text` into at most `max_lines` lines no wider than
/// `width`; text beyond the last line is cut off with an ellipsis.
pub fn wrap(text: &str, style: TextStyle, width: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if style.width(&format!("{line} {word}")) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            last.push('…');
        }
    }
    lines
}

/// Draws lines top-down from the top of `rect`, each centered in its
/// own line box of `line_height`.
pub fn paragraph(rect: Rect, lines: &[String], style: TextStyle, line_height: f32) {
    let mut rest = rect;
    for line in lines {
        label(rest.take_top(line_height, 0.0), line, style, Align::Left);
    }
}

// --- shapes ---

pub fn rounded_rect(rect: Rect, radius: f32, color: Color) {
    let r = radius.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
    draw_rectangle(rect.x + r, rect.y, rect.w - 2.0 * r, rect.h, color);
    draw_rectangle(rect.x, rect.y + r, rect.w, rect.h - 2.0 * r, color);
    for (x, y) in [
        (rect.x + r, rect.y + r),
        (rect.right() - r, rect.y + r),
        (rect.x + r, rect.bottom() - r),
        (rect.right() - r, rect.bottom() - r),
    ] {
        draw_circle(x, y, r, color);
    }
}

/// A rounded card; a border is drawn inside its edge, following the
/// rounded corners.
pub fn card(rect: Rect, radius: f32, fill: Color, border: Option<(f32, Color)>) {
    match border {
        Some((width, color)) => {
            rounded_rect(rect, radius, color);
            rounded_rect(rect.inset(width), radius - width, fill);
        }
        None => rounded_rect(rect, radius, fill),
    }
}

/// A square outline `width` thick, inside `rect`.
pub fn inner_outline(rect: Rect, width: f32, color: Color) {
    // macroquad strokes half of its `thickness` inside the rectangle.
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, width * 2.0, color);
}

// --- rectangles ---

pub trait RectExt: Sized {
    fn inset(self, by: f32) -> Rect;
    fn inset_xy(self, dx: f32, dy: f32) -> Rect;
    /// Cuts `height` off the top; the remainder starts `gap` below it.
    fn take_top(&mut self, height: f32, gap: f32) -> Rect;
    /// Cuts `width` off the left; the remainder starts `gap` right of it.
    fn take_left(&mut self, width: f32, gap: f32) -> Rect;
    /// Cuts `width` off the right; the remainder ends `gap` left of it.
    fn take_right(&mut self, width: f32, gap: f32) -> Rect;
    /// `n` equal columns separated by `gap`.
    fn columns(self, n: usize, gap: f32) -> Vec<Rect>;
    /// A `width` × `height` rectangle centered horizontally at the top.
    fn top_center(self, width: f32, height: f32) -> Rect;
    /// A full-width slice `height` tall, centered vertically.
    fn middle(self, height: f32) -> Rect;
}

impl RectExt for Rect {
    fn inset(self, by: f32) -> Rect {
        self.inset_xy(by, by)
    }

    fn inset_xy(self, dx: f32, dy: f32) -> Rect {
        Rect::new(
            self.x + dx,
            self.y + dy,
            self.w - 2.0 * dx,
            self.h - 2.0 * dy,
        )
    }

    fn take_top(&mut self, height: f32, gap: f32) -> Rect {
        let top = Rect::new(self.x, self.y, self.w, height);
        self.y += height + gap;
        self.h -= height + gap;
        top
    }

    fn take_left(&mut self, width: f32, gap: f32) -> Rect {
        let left = Rect::new(self.x, self.y, width, self.h);
        self.x += width + gap;
        self.w -= width + gap;
        left
    }

    fn take_right(&mut self, width: f32, gap: f32) -> Rect {
        let right = Rect::new(self.right() - width, self.y, width, self.h);
        self.w -= width + gap;
        right
    }

    fn columns(self, n: usize, gap: f32) -> Vec<Rect> {
        let width = (self.w - gap * (n.saturating_sub(1)) as f32) / n as f32;
        (0..n)
            .map(|i| Rect::new(self.x + i as f32 * (width + gap), self.y, width, self.h))
            .collect()
    }

    fn top_center(self, width: f32, height: f32) -> Rect {
        Rect::new(self.x + (self.w - width) / 2.0, self.y, width, height)
    }

    fn middle(self, height: f32) -> Rect {
        Rect::new(self.x, self.y + (self.h - height) / 2.0, self.w, height)
    }
}

// --- screens ---

pub fn draw(play: &Match, layout: &Layout) {
    clear_background(palette::BACKGROUND);
    let presence = play
        .my_side()
        .zip(play.opponent_online())
        .map(|(me, opponent_online)| Presence {
            me,
            opponent_online,
        });
    hud::header(layout.header, presence);
    match (play.game(), &layout.play_area) {
        (Some(game), Some(PlayArea { board, sidebar })) => {
            // Hints reveal what the current player can do: always in
            // hotseat, online only on our own turn.
            let show_hints = play
                .my_side()
                .is_none_or(|side| side == game.current_player());
            board::draw(game, board, show_hints, play.my_side());
            hud::sidebar(play, game, sidebar);
        }
        // Online, the second seat waits briefly for the first seat's
        // board.
        _ => message(
            layout,
            "Starting…",
            "Waiting for the board from the other player.",
        ),
    }
}

/// A screen with only a message: the native relay's waiting screen
/// (on the web, the page does the waiting, in HTML).
#[cfg(not(target_arch = "wasm32"))]
pub fn message_screen(title: &str, body: &str) {
    clear_background(palette::BACKGROUND);
    let layout = Layout::compute(None);
    hud::header(layout.header, None);
    message(&layout, title, body);
}

fn message(layout: &Layout, title: &str, body: &str) {
    hud::text_card(layout.message(), None, title, body);
}
