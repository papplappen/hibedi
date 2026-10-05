//! The board: cells, hint markers and tokens, plus mapping clicks back
//! to cells.

use hibedi_core::{Cell, Coord, Game, Side};
use macroquad::prelude::*;

use super::palette;
use super::{RectExt, inner_outline, rounded_rect, side_color};

/// Where the board sits: a frame around a grid of square cells.
pub struct BoardLayout {
    /// The board including its frame.
    pub outer: Rect,
    cell_size: f32,
    columns: usize,
    rows: usize,
}

impl BoardLayout {
    /// Width of the frame drawn around the cells.
    pub const FRAME: f32 = 6.0;

    /// The board for a `columns` × `rows` grid of `cell_size` cells,
    /// with its frame's top-left corner at `origin`.
    pub fn new(origin: Vec2, cell_size: f32, columns: usize, rows: usize) -> Self {
        let size = Self::outer_size(cell_size, columns, rows);
        Self {
            outer: Rect::new(origin.x, origin.y, size.x, size.y),
            cell_size,
            columns,
            rows,
        }
    }

    /// Size of the board, frame included.
    pub fn outer_size(cell_size: f32, columns: usize, rows: usize) -> Vec2 {
        vec2(columns as f32, rows as f32) * cell_size + Vec2::splat(2.0 * Self::FRAME)
    }

    fn cells(&self) -> Rect {
        self.outer.inset(Self::FRAME)
    }

    fn cell_rect(&self, coord: Coord) -> Rect {
        let cells = self.cells();
        Rect::new(
            cells.x + coord.x as f32 * self.cell_size,
            cells.y + coord.y as f32 * self.cell_size,
            self.cell_size,
            self.cell_size,
        )
    }

    pub fn coord_at(&self, point: Vec2) -> Option<Coord> {
        let cells = self.cells();
        if !cells.contains(point) {
            return None;
        }
        let local = (point - cells.point()) / self.cell_size;
        let coord = Coord::new(local.x.floor() as i32, local.y.floor() as i32);
        // `contains` includes the far edges; keep them inside the grid.
        ((coord.x as usize) < self.columns && (coord.y as usize) < self.rows).then_some(coord)
    }
}

pub fn draw(game: &Game, layout: &BoardLayout, show_hints: bool, my_side: Option<Side>) {
    let board = game.board();
    let placements = game.legal_initial_placements();
    let jumps = game.legal_current_jumps();
    let selectable = game.selectable_activated_tokens();

    rounded_rect(layout.outer, 14.0, palette::PANEL);

    for y in 0..board.storage_height() as i32 {
        for x in 0..board.storage_width() as i32 {
            let coord = Coord::new(x, y);
            let rect = layout.cell_rect(coord);
            let tile = rect.inset(1.0);
            let cell = board.cell_at(coord).unwrap_or(Cell::Empty);
            draw_rectangle(
                tile.x,
                tile.y,
                tile.w,
                tile.h,
                cell.side().map_or(palette::EMPTY_CELL, side_color),
            );

            if game.is_crossed_this_chain(coord) {
                draw_rectangle(tile.x, tile.y, tile.w, tile.h, palette::USED_CELL_OVERLAY);
            }

            // The viewer's own territory (online): a light inner frame.
            if my_side.is_some() && cell.side() == my_side {
                let inset = (rect.w * 0.05).clamp(2.0, 5.0);
                let width = (rect.w * 0.0275).clamp(1.5, 3.0);
                inner_outline(rect.inset(inset), width, palette::OWN_EDGE);
            }

            if show_hints && placements.contains(&coord) {
                marker(
                    rect,
                    palette::CYAN,
                    Marker {
                        inset: 0.11,
                        line: 0.04,
                        dot: 0.10,
                    },
                );
            }
            if show_hints && jumps.iter().any(|jump| jump.landing == coord) {
                marker(
                    rect,
                    palette::GOLD,
                    Marker {
                        inset: 0.09,
                        line: 0.05,
                        dot: 0.13,
                    },
                );
            }
        }
    }

    for token in board.tokens() {
        let rect = layout.cell_rect(token.position);
        let center = rect.center();
        let radius = rect.w * 0.27;

        if show_hints && selectable.contains(&token.id) {
            draw_circle(center.x, center.y, radius + 7.0, palette::GOLD);
        } else if game.pending_activated_tokens().contains(&token.id) {
            draw_circle_lines(center.x, center.y, radius + 5.0, 2.0, palette::MUTED);
        }

        // The token currently jumping wears a thick cyan rim; the others
        // a thin light one.
        let (rim, color) = if game.current_token() == Some(token.id) {
            ((rect.w * 0.07).clamp(3.0, 6.0), palette::CYAN)
        } else {
            (2.0, palette::TOKEN_EDGE)
        };
        draw_circle(center.x, center.y, radius + rim / 2.0, color);
        draw_circle(center.x, center.y, radius - rim / 2.0, palette::TOKEN_FILL);
    }
}

/// A hint marker's proportions, as fractions of the cell size.
struct Marker {
    inset: f32,
    line: f32,
    dot: f32,
}

/// Hint marker: a square frame with a center dot.
fn marker(cell: Rect, color: Color, marker: Marker) {
    let size = cell.w;
    let inset = (size * marker.inset).clamp(2.0, 6.0);
    inner_outline(
        cell.inset(inset),
        (size * marker.line).clamp(1.5, 2.75),
        color,
    );
    let center = cell.center();
    draw_circle(center.x, center.y, size * marker.dot, color);
}
