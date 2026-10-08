//! Sketching moves, to try them out without making them: the right mouse
//! button draws a line along a row or column, and the cells under it
//! show flipped, as a jump along it would leave them. A line is dragged
//! out (press, move, let go), or clicked out (click where it starts,
//! click where it ends); either way a pale preview follows the pointer
//! until it ends.
//!
//! Lines add up: a cell under two of them flips back. Drawing a line
//! again erases it; a left click, or any change to the game, clears the
//! sketch. The board shows it; only the game counts.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use hibedi_core::{Coord, Game};

use super::BoardGrid;
use crate::play::Match;
use crate::ui::Overlay;

pub fn plugin(app: &mut App) {
    app.init_resource::<Sketch>().add_systems(
        PreUpdate,
        (
            clear_on_change.run_if(resource_changed::<Match>),
            draw.after(bevy::input::InputSystems)
                .run_if(in_state(Overlay::None)),
        )
            .chain(),
    );
}

#[derive(Resource, Default)]
pub struct Sketch {
    pub lines: Vec<Line>,
    /// The line being drawn, from where it started to the pointer.
    pub drawing: Option<Line>,
}

impl Sketch {
    /// Whether the lines flip the cell at `coord` (the line being drawn
    /// does not count until it is done).
    pub fn flips(&self, coord: Coord) -> bool {
        self.lines.iter().filter(|line| line.covers(coord)).count() % 2 == 1
    }
}

/// A straight run of cells along a row or column, both ends included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub from: Coord,
    pub to: Coord,
}

impl Line {
    pub fn covers(self, coord: Coord) -> bool {
        let between = |a: i32, b: i32, value: i32| a.min(b) <= value && value <= a.max(b);
        between(self.from.x, self.to.x, coord.x) && between(self.from.y, self.to.y, coord.y)
    }
}

fn clear_on_change(play: Res<Match>, mut sketch: ResMut<Sketch>, mut seen: Local<Option<Game>>) {
    if play.game() != seen.as_ref() {
        *seen = play.game().cloned();
        if !sketch.lines.is_empty() {
            sketch.lines.clear();
        }
    }
}

fn draw(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    grid: Option<Single<(&BoardGrid, &ComputedNode, &UiGlobalTransform)>>,
    mut sketch: ResMut<Sketch>,
) {
    // Read before writing: writing marks the sketch changed.
    if mouse.just_pressed(MouseButton::Left)
        && (!sketch.lines.is_empty() || sketch.drawing.is_some())
    {
        sketch.lines.clear();
        sketch.drawing = None;
    }

    // Where the pointer is on the grid, in logical pixels. UI nodes are
    // laid out in physical pixels; normalized, a node spans -0.5 to 0.5.
    let pointer = grid.and_then(|grid| {
        let (grid, node, transform) = *grid;
        let at = node.normalize_point(*transform, window.physical_cursor_position()?)? + 0.5;
        Some((grid, at * grid.size()))
    });

    // The line follows the pointer along the row or the column, whichever
    // is closer.
    if let Some(drawing) = sketch.drawing
        && let Some((grid, at)) = pointer
    {
        let pointer = grid.nearest_cell(at);
        let (dx, dy) = (pointer.x - drawing.from.x, pointer.y - drawing.from.y);
        let to = if dx.abs() >= dy.abs() {
            Coord::new(pointer.x, drawing.from.y)
        } else {
            Coord::new(drawing.from.x, pointer.y)
        };
        if to != drawing.to {
            sketch.drawing = Some(Line { to, ..drawing });
        }
    }

    if mouse.just_pressed(MouseButton::Right) {
        match sketch.drawing {
            // A second click ends a clicked line.
            Some(line) => finish(&mut sketch, line),
            None => {
                if let Some(from) = pointer.and_then(|(grid, at)| grid.cell_at(at)) {
                    sketch.drawing = Some(Line { from, to: from });
                }
            }
        }
    }
    // Let go away from where it started, a line was dragged out: it ends
    // there. Let go where it started, it was a click: it goes on.
    if mouse.just_released(MouseButton::Right)
        && let Some(line) = sketch.drawing
        && line.to != line.from
    {
        finish(&mut sketch, line);
    }
}

/// Ends the line being drawn: it toggles, unless it never left its
/// first cell.
fn finish(sketch: &mut Sketch, line: Line) {
    sketch.drawing = None;
    if line.from == line.to {
        return;
    }
    if let Some(index) = sketch.lines.iter().position(|&drawn| drawn == line) {
        sketch.lines.remove(index);
    } else {
        sketch.lines.push(line);
    }
}
