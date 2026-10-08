//! Sketching moves, to try them out without making them: dragging with
//! the right mouse button draws a line along a row or column, and the
//! cells under it show flipped, as a jump along it would leave them.
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
    /// The line being dragged out.
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
    if mouse.just_pressed(MouseButton::Left) && !sketch.lines.is_empty() {
        sketch.lines.clear();
    }

    // Where the pointer is on the grid, from 0 to 1 each way. UI nodes
    // are laid out in physical pixels; normalized, a node spans -0.5 to
    // 0.5.
    let pointer = grid.and_then(|grid| {
        let (grid, node, transform) = *grid;
        let at = node.normalize_point(*transform, window.physical_cursor_position()?)? + 0.5;
        Some((grid.columns, grid.rows, at))
    });
    let cell = |(columns, rows, at): (usize, usize, Vec2)| {
        let column = (at.x * columns as f32).floor() as i32;
        let row = (at.y * rows as f32).floor() as i32;
        Coord::new(
            column.clamp(0, columns as i32 - 1),
            row.clamp(0, rows as i32 - 1),
        )
    };

    if mouse.just_pressed(MouseButton::Right)
        && let Some(pointer @ (_, _, at)) = pointer
        && at.cmpge(Vec2::ZERO).all()
        && at.cmplt(Vec2::ONE).all()
    {
        let from = cell(pointer);
        sketch.drawing = Some(Line { from, to: from });
    }

    // The line follows the pointer along the row or the column, whichever
    // is closer.
    if let Some(drawing) = sketch.drawing
        && let Some(pointer) = pointer
    {
        let pointer = cell(pointer);
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

    // Read before writing: writing marks the sketch changed.
    if !mouse.pressed(MouseButton::Right)
        && let Some(line) = sketch.drawing
    {
        sketch.drawing = None;
        if let Some(index) = sketch.lines.iter().position(|&drawn| drawn == line) {
            sketch.lines.remove(index);
        } else {
            sketch.lines.push(line);
        }
    }
}
