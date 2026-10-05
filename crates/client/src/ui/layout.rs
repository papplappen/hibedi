//! Screen layout, recomputed every frame from the window size.
//!
//! A header band runs along the top (title left, connection status
//! right). Below it the board and the sidebar form one group, either
//! side by side or stacked (sidebar under the board), whichever gives
//! the board larger cells.

use hibedi_core::Game;
use macroquad::prelude::*;

use super::{BoardLayout, RectExt};

/// Screen margin; [`COMPACT_PAD`] on small screens.
const PAD: f32 = 24.0;
const COMPACT_PAD: f32 = 16.0;
/// Screens whose short side is below this use the compact margin.
const COMPACT_BELOW: f32 = 600.0;
pub const HEADER_H: f32 = 44.0;
/// Gap between the board and the sidebar, and between cards.
pub const GAP: f32 = 16.0;
/// Sidebar width beside the board.
const SIDEBAR_W: f32 = 280.0;
/// Widest a stacked sidebar or a message card grows.
const WIDE_CARD_W: f32 = 460.0;
/// Below this cell size the board is not worth laying out larger.
const MIN_CELL: f32 = 12.0;

/// The sidebar's cards, top to bottom.
pub struct Sidebar {
    pub score: Rect,
    pub turn: Rect,
    /// The "new game" button's slot; kept whether or not the button is
    /// shown, so nothing moves when it appears.
    pub button: Rect,
}

impl Sidebar {
    pub const SCORE_H: f32 = 64.0;
    pub const BUTTON_H: f32 = 44.0;
    pub const HEIGHT: f32 = Self::SCORE_H + GAP + text_card::HEIGHT + GAP + Self::BUTTON_H;

    fn split(mut rect: Rect) -> Self {
        Self {
            score: rect.take_top(Self::SCORE_H, GAP),
            turn: rect.take_top(text_card::HEIGHT, GAP),
            button: rect.take_top(Self::BUTTON_H, 0.0),
        }
    }
}

/// Metrics of a card with a title line and a short wrapped body: the
/// turn card and messages.
pub mod text_card {
    pub const PAD: f32 = 18.0;
    pub const TITLE_LINE: f32 = 24.0;
    pub const TITLE_GAP: f32 = 8.0;
    pub const BODY_LINE: f32 = 21.0;
    pub const BODY_LINES: usize = 3;
    pub const HEIGHT: f32 = 2.0 * PAD + TITLE_LINE + TITLE_GAP + BODY_LINES as f32 * BODY_LINE;
}

pub struct Layout {
    pub header: Rect,
    /// Everything below the header.
    pub content: Rect,
    /// Board and sidebar, when there is a game.
    pub play_area: Option<PlayArea>,
}

pub struct PlayArea {
    pub board: BoardLayout,
    pub sidebar: Sidebar,
}

impl Layout {
    pub fn compute(game: Option<&Game>) -> Self {
        let screen = Rect::new(0.0, 0.0, screen_width(), screen_height());
        let pad = if screen.w.min(screen.h) < COMPACT_BELOW {
            COMPACT_PAD
        } else {
            PAD
        };
        let mut content = screen.inset(pad);
        let header = content.take_top(HEADER_H, pad);
        let play_area = game.map(|game| {
            let board = game.board();
            PlayArea::fit(content, board.storage_width(), board.storage_height())
        });
        Self {
            header,
            content,
            play_area,
        }
    }

    /// Where a message card goes when there is no board.
    pub fn message(&self) -> Rect {
        self.content
            .top_center(self.content.w.min(WIDE_CARD_W), text_card::HEIGHT)
    }
}

impl PlayArea {
    fn fit(content: Rect, columns: usize, rows: usize) -> Self {
        let frame = 2.0 * BoardLayout::FRAME;
        // Largest whole-pixel cell for a board `width` × `height` (frame
        // included) can occupy.
        let cell_for = |width: f32, height: f32| {
            ((width - frame) / columns as f32)
                .min((height - frame) / rows as f32)
                .floor()
                .max(MIN_CELL)
        };

        let side_cell = cell_for(content.w - GAP - SIDEBAR_W, content.h);
        let stacked_cell = cell_for(content.w, content.h - GAP - Sidebar::HEIGHT);

        if side_cell >= stacked_cell {
            let board_size = BoardLayout::outer_size(side_cell, columns, rows);
            let mut group = content.top_center(board_size.x + GAP + SIDEBAR_W, content.h);
            let board = group.take_left(board_size.x, GAP);
            Self {
                board: BoardLayout::new(board.point(), side_cell, columns, rows),
                sidebar: Sidebar::split(group),
            }
        } else {
            let board_size = BoardLayout::outer_size(stacked_cell, columns, rows);
            let mut column = content;
            let board = column
                .take_top(board_size.y, GAP)
                .top_center(board_size.x, board_size.y);
            Self {
                board: BoardLayout::new(board.point(), stacked_cell, columns, rows),
                sidebar: Sidebar::split(
                    column.top_center(column.w.min(WIDE_CARD_W), Sidebar::HEIGHT),
                ),
            }
        }
    }
}
