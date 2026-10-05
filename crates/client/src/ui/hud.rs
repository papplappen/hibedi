//! Everything around the board: the header with the connection status,
//! the sidebar (score, whose turn and what to do, new game), and message
//! cards.

use hibedi_core::{Game, GameResult, Phase, Side, WinReason};
use macroquad::prelude::*;

use super::layout::{GAP, HEADER_H, Sidebar, text_card};
use super::palette;
use super::{Align, RectExt, TextStyle, card, label, paragraph, rounded_rect, side_color, wrap};
use crate::play::Match;

/// Who we are and whether the other player is here (online only).
#[derive(Clone, Copy)]
pub struct Presence {
    pub me: Side,
    pub opponent_online: bool,
}

const TITLE: &str = "HIBE DI HOPEDI";
const SUBTITLE: &str = "A forced-hop territory prototype";
/// Title sizes from largest to smallest; the largest that fits is used.
const TITLE_SIZES: [u16; 3] = [26, 22, 18];
const TITLE_LINE: f32 = 26.0;
const SUBTITLE_STYLE: TextStyle = TextStyle::regular(14, palette::MUTED);

const CARD_RADIUS: f32 = 12.0;
const CARD_BORDER: Option<(f32, Color)> = Some((1.0, palette::RAISED));
const CARD_TITLE: TextStyle = TextStyle::bold(18, palette::TEXT);
const CARD_BODY: TextStyle = TextStyle::regular(15, palette::MUTED);
/// Diameter of the color swatches before titles and names.
const SWATCH: f32 = 12.0;

const PILL_H: f32 = 32.0;
const PILL_PAD: f32 = 12.0;
const PILL_TEXT: TextStyle = TextStyle::bold(14, palette::TEXT);
const SEAT_DOT: f32 = 10.0;
const SEAT_GAP: f32 = 6.0;

const SCORE_PAD: f32 = 8.0;
const NAME: TextStyle = TextStyle::bold(16, palette::TEXT);
const NAME_LINE: f32 = 20.0;
const YOU: TextStyle = TextStyle::bold(11, palette::MUTED);
const YOU_LINE: f32 = 14.0;
const COUNT: TextStyle = TextStyle::bold(24, palette::TEXT);

const BUTTON: TextStyle = TextStyle::bold(15, palette::TEXT);

// --- header ---

pub fn header(rect: Rect, presence: Option<Presence>) {
    let mut rest = rect;
    if let Some(presence) = presence {
        let slot = rest.take_right(pill_width(presence), GAP);
        pill(presence, slot.inset_xy(0.0, (HEADER_H - PILL_H) / 2.0));
    }

    let Some(size) = TITLE_SIZES
        .into_iter()
        .find(|&size| TextStyle::bold(size, palette::TEXT).width(TITLE) <= rest.w)
    else {
        return;
    };
    let title = TextStyle::bold(size, palette::TEXT);
    if size == TITLE_SIZES[0] && SUBTITLE_STYLE.width(SUBTITLE) <= rest.w {
        label(rest.take_top(TITLE_LINE, 0.0), TITLE, title, Align::Left);
        label(rest, SUBTITLE, SUBTITLE_STYLE, Align::Left);
    } else {
        label(rest, TITLE, title, Align::Left);
    }
}

/// The players at a glance: one dot per side, ours ringed white, the
/// opponent's filled while online and hollow while away.
fn pill(presence: Presence, rect: Rect) {
    card(rect, PILL_H / 2.0, palette::PANEL, CARD_BORDER);
    let mut inner = rect.inset_xy(PILL_PAD, 0.0);
    for side in Side::ALL {
        let center = inner.take_left(SEAT_DOT, SEAT_GAP).center();
        let color = side_color(side);
        if side == presence.me || presence.opponent_online {
            draw_circle(center.x, center.y, SEAT_DOT / 2.0, color);
        } else {
            draw_circle_lines(center.x, center.y, SEAT_DOT / 2.0 - 1.0, 2.0, color);
        }
        if side == presence.me {
            draw_circle_lines(
                center.x,
                center.y,
                SEAT_DOT / 2.0 + 2.5,
                1.5,
                palette::OWN_EDGE,
            );
        }
    }
    label(
        inner.inset_xy(SEAT_GAP, 0.0),
        pill_text(presence),
        PILL_TEXT,
        Align::Left,
    );
}

fn pill_width(presence: Presence) -> f32 {
    let dots = Side::ALL.len() as f32 * (SEAT_DOT + SEAT_GAP);
    2.0 * PILL_PAD + dots + SEAT_GAP + PILL_TEXT.width(pill_text(presence))
}

fn pill_text(presence: Presence) -> &'static str {
    if presence.opponent_online {
        "Opponent online"
    } else {
        "Opponent offline"
    }
}

// --- sidebar ---

pub fn sidebar(play: &Match, game: &Game, sidebar: &Sidebar) {
    score(play, game, sidebar.score);
    let turn = turn_text(play, game);
    text_card(sidebar.turn, turn.accent, &turn.title, &turn.body);
    if play.can_start_new_game() {
        button(sidebar.button, "New game", game.result().is_some());
    }
}

/// Colored-cell counts per side; our own side (online) is highlighted.
fn score(play: &Match, game: &Game, rect: Rect) {
    card(rect, CARD_RADIUS, palette::PANEL, CARD_BORDER);
    let tiles = rect.inset(SCORE_PAD).columns(Side::ALL.len(), SCORE_PAD);
    for (side, tile) in Side::ALL.into_iter().zip(tiles) {
        let count = game.board().colored_count(side);
        let mine = play.my_side() == Some(side);
        if mine {
            rounded_rect(tile, CARD_RADIUS - SCORE_PAD / 2.0, palette::RAISED);
        }
        let mut inner = tile.inset_xy(12.0, 0.0);
        label(inner, &count.to_string(), COUNT, Align::Right);

        let swatch = inner.take_left(SWATCH, 8.0).center();
        draw_circle(swatch.x, swatch.y, SWATCH / 2.0, side_color(side));
        if mine {
            let mut lines = inner.middle(NAME_LINE + YOU_LINE);
            label(
                lines.take_top(NAME_LINE, 0.0),
                &side.to_string(),
                NAME,
                Align::Left,
            );
            label(lines, "YOU", YOU, Align::Left);
        } else {
            label(inner, &side.to_string(), NAME, Align::Left);
        }
    }
}

struct TurnText {
    accent: Option<Color>,
    title: String,
    body: String,
}

fn turn_text(play: &Match, game: &Game) -> TurnText {
    let text = |side: Option<Side>, title: String, body: String| TurnText {
        accent: side.map(side_color),
        title,
        body,
    };

    if let Some(result) = game.result() {
        return match result {
            GameResult::Winner { side, reason } => text(
                Some(side),
                format!("{side} wins"),
                match reason {
                    WinReason::Monochrome => {
                        format!("Every colored cell is {}.", side.to_string().to_lowercase())
                    }
                    WinReason::Majority => format!(
                        "No moves are left, and {} holds more colored cells.",
                        side.to_string().to_lowercase()
                    ),
                },
            ),
            GameResult::Draw => text(
                None,
                "Draw".to_owned(),
                "No moves are left, and both colors hold equally many cells.".to_owned(),
            ),
        };
    }

    let current = game.current_player();
    let to_move = format!("{current} to move");
    match play.opponent_online() {
        None => text(Some(current), to_move, instruction(game).to_owned()),
        Some(false) => text(
            Some(current),
            to_move,
            "Your opponent is offline. The game continues when they are back.".to_owned(),
        ),
        Some(true) if play.my_side() == Some(current) => text(
            Some(current),
            "Your move".to_owned(),
            instruction(game).to_owned(),
        ),
        Some(true) => text(
            Some(current),
            to_move,
            "Waiting for your opponent.".to_owned(),
        ),
    }
}

/// What the player to move does next.
fn instruction(game: &Game) -> &'static str {
    match game.phase() {
        Phase::PlaceInitialToken => "Place a token on a cyan-marked cell.",
        Phase::ChooseJump if !game.selectable_activated_tokens().is_empty() => {
            "Jump to a gold-marked cell, or switch to another glowing token."
        }
        Phase::ChooseJump => "Jump to a gold-marked cell. Every available jump is compulsory.",
        Phase::ChooseActivatedToken => "Choose a glowing token to jump with.",
        Phase::GameOver => "",
    }
}

fn button(rect: Rect, text: &str, primary: bool) {
    // Dark text on gold: light text lacks contrast against it.
    let (fill, color) = if primary {
        (palette::GOLD, palette::BACKGROUND)
    } else {
        (palette::RAISED, palette::TEXT)
    };
    rounded_rect(rect, 10.0, fill);
    label(rect, text, BUTTON.with_color(color), Align::Center);
}

// --- shared ---

/// A card with an optional color swatch, a title line, and a wrapped
/// body (see [`TextCard`] for its fixed metrics).
pub fn text_card(rect: Rect, accent: Option<Color>, title: &str, body: &str) {
    card(rect, CARD_RADIUS, palette::PANEL, CARD_BORDER);
    let mut inner = rect.inset(text_card::PAD);
    let mut title_row = inner.take_top(text_card::TITLE_LINE, text_card::TITLE_GAP);
    if let Some(color) = accent {
        let swatch = title_row.take_left(SWATCH, 10.0).center();
        draw_circle(swatch.x, swatch.y, SWATCH / 2.0, color);
    }
    label(title_row, title, CARD_TITLE, Align::Left);
    let lines = wrap(body, CARD_BODY, inner.w, text_card::BODY_LINES);
    paragraph(inner, &lines, CARD_BODY, text_card::BODY_LINE);
}
