//! Where each cell of a grid sits on the board.
//!
//! A cell's place is worked out in *lattice units* — one cell on a
//! square grid, one hex radius on a hex lattice — and turned into
//! percentages of the board box. The box is exactly as big as the grid's
//! outer ring, so no cell is drawn half off the board, and every number
//! is a ratio, so the same layout holds at any window size.

use bevy::math::{Vec2, vec2};
use hibedi_core::{Coord, Grid};

/// The width of a hex across, for its radius: the root of three.
const HEX_WIDTH: f32 = 1.732_050_8;
/// The width of a hex for its height, so a square decoration on a hex
/// cell stays square.
const HEX_WIDTH_FOR_HEIGHT: f32 = 0.866_025_4;
/// The sideways shift of a hex row for its radius: half a cell.
const HEX_ROW_SHIFT: f32 = 0.5;
/// How far one hex row sits below the next, for its radius.
const HEX_ROW_STEP: f32 = 1.5;
/// The gap left between neighbouring cells, per side, as a fraction of
/// a cell.
pub const GAP: f32 = 0.015;

/// A [`Grid`]'s geometry: where every cell is and how big.
#[derive(Clone, Copy, Debug)]
pub struct Lattice {
    grid: Grid,
    /// The corner the board box starts at, in lattice units.
    origin: Vec2,
    /// The board box's size, in lattice units.
    span: Vec2,
    /// One cell's box, in lattice units.
    cell: Vec2,
}

impl Lattice {
    pub fn new(grid: Grid) -> Self {
        let cell = match grid {
            Grid::Square { .. } => Vec2::ONE,
            Grid::Hex { .. } => vec2(HEX_WIDTH, 2.0),
        };
        let mut origin = Vec2::splat(f32::INFINITY);
        let mut far = Vec2::splat(f32::NEG_INFINITY);
        for coord in grid.storage_coords() {
            let here = units(grid, coord);
            origin = origin.min(here);
            far = far.max(here);
        }

        Self {
            grid,
            origin,
            // From the top left corner of the first cell to the bottom
            // right corner of the last, so the box holds them all.
            span: far - origin + cell,
            cell,
        }
    }

    pub const fn grid(&self) -> Grid {
        self.grid
    }

    /// A cell's top left corner, in percent of the board box.
    pub fn anchor(&self, coord: Coord) -> Vec2 {
        (units(self.grid, coord) - self.origin) / self.span * 100.0
    }

    /// One cell's box, in percent of the board box.
    pub fn cell(&self) -> Vec2 {
        self.cell / self.span * 100.0
    }

    /// A cell's box, top left corner and size, pulled in by [`GAP`] so
    /// neighbouring cells do not touch.
    pub fn inlay(&self, coord: Coord) -> (Vec2, Vec2) {
        let cell = self.cell();
        (self.anchor(coord) + cell * GAP, cell * (1.0 - 2.0 * GAP))
    }

    /// The board box for cells of size `size` — a hex's radius, a
    /// square cell's side — in pixels.
    pub fn extent(&self, size: f32) -> Vec2 {
        self.span * size
    }

    /// The largest whole `size` whose board box fits `available`, never
    /// below `min`.
    pub fn fit(&self, available: Vec2, min: f32) -> f32 {
        (available.x / self.span.x)
            .min(available.y / self.span.y)
            .floor()
            .max(min)
    }

    /// How far apart two cells are, in cells: how long a hop across
    /// them is.
    pub fn distance(&self, from: Coord, to: Coord) -> f32 {
        (units(self.grid, from) - units(self.grid, to)).length()
    }

    /// A decoration `size` percent of a cell's width, kept square on
    /// both grids so a circle stays a circle.
    pub fn square(&self, size: f32) -> Vec2 {
        let height_for_width = match self.grid {
            Grid::Square { .. } => 1.0,
            Grid::Hex { .. } => HEX_WIDTH_FOR_HEIGHT,
        };
        vec2(size, size * height_for_width)
    }
}

/// A cell's place in lattice units, measured from the playable area's
/// top left corner. Hex cells step sideways along the diagonals, every
/// row half a cell over.
fn units(grid: Grid, coord: Coord) -> Vec2 {
    let (x, y) = (coord.x - 1, coord.y - 1);
    match grid {
        Grid::Square { .. } => vec2(x as f32, y as f32),
        Grid::Hex { .. } => {
            let (q, r) = (x as f32, y as f32);
            vec2(HEX_WIDTH * (q + HEX_ROW_SHIFT * r), HEX_ROW_STEP * r)
        }
    }
}

#[cfg(test)]
mod tests {
    use hibedi_core::Direction;

    use super::*;

    const EPSILON: f32 = 0.001;

    fn every_lattice() -> [Lattice; 2] {
        [Lattice::new(Grid::SQUARE), Lattice::new(Grid::HEX)]
    }

    fn assert_close(actual: Vec2, expected: Vec2) {
        assert!(
            (actual - expected).abs().max_element() < EPSILON,
            "{actual:?} should be {expected:?}"
        );
    }

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < EPSILON,
            "{actual} should be {expected}"
        );
    }

    #[test]
    fn every_cell_sits_inside_the_board_box() {
        for lattice in every_lattice() {
            let cell = lattice.cell();
            for coord in lattice.grid().storage_coords() {
                let anchor = lattice.anchor(coord);
                assert!(anchor.x >= -EPSILON && anchor.y >= -EPSILON, "{coord:?}");
                assert!(
                    anchor.x + cell.x <= 100.0 + EPSILON && anchor.y + cell.y <= 100.0 + EPSILON,
                    "{coord:?} runs off the board at {anchor:?}"
                );
            }
        }
    }

    #[test]
    fn the_box_is_as_big_as_the_cells() {
        for lattice in every_lattice() {
            let cell = lattice.cell();
            let corners = lattice
                .grid()
                .storage_coords()
                .flat_map(|coord| {
                    let anchor = lattice.anchor(coord);
                    [anchor, anchor + cell]
                })
                .fold(
                    (Vec2::splat(f32::INFINITY), Vec2::splat(-f32::INFINITY)),
                    |(min, max), at| (min.min(at), max.max(at)),
                );

            assert_close(corners.0, Vec2::ZERO);
            assert_close(corners.1, Vec2::splat(100.0));
        }
    }

    #[test]
    fn neighbours_are_equally_far_apart() {
        let square = Lattice::new(Grid::SQUARE);
        let hex = Lattice::new(Grid::HEX);
        let middle = Coord::new(2, 2);

        for lattice in [square, hex] {
            let distances: Vec<f32> = lattice
                .grid()
                .directions()
                .iter()
                .map(|&direction| {
                    lattice.distance(middle, middle.shifted(lattice.grid(), direction))
                })
                .collect();
            let first = distances[0];
            assert!(
                distances.iter().all(|&d| (d - first).abs() < EPSILON),
                "a {lattice:?} grid has uneven steps: {distances:?}"
            );
        }

        // One cell on a square grid, one hex edge on a hex lattice.
        let right = middle.shifted(Grid::SQUARE, Direction::Right);
        close(square.distance(middle, right), 1.0);
        let diagonal = middle.shifted(Grid::HEX, Direction::DownRight);
        close(
            hex.distance(middle, diagonal),
            vec2(HEX_WIDTH_FOR_HEIGHT, HEX_ROW_STEP).length(),
        );
    }

    #[test]
    fn a_square_grid_lays_out_in_even_columns() {
        let lattice = Lattice::new(Grid::Square {
            width: 4,
            height: 5,
        });
        let (columns, rows) = (6.0, 7.0);

        assert_close(lattice.cell(), vec2(100.0 / columns, 100.0 / rows));
        assert_close(lattice.anchor(Coord::new(0, 0)), Vec2::ZERO);
        assert_close(
            lattice.anchor(Coord::new(1, 1)),
            vec2(100.0 / columns, 100.0 / rows),
        );
        assert_close(lattice.extent(10.0), vec2(columns * 10.0, rows * 10.0));
    }

    #[test]
    fn hex_rows_sit_half_a_cell_over() {
        let lattice = Lattice::new(Grid::HEX);
        let column = lattice.anchor(Coord::new(1, 1));
        let next_row = lattice.anchor(Coord::new(1, 2));

        // A row down is one hex row, and half a cell sideways.
        assert_close(
            next_row - column,
            vec2(
                lattice.cell().x * HEX_ROW_SHIFT,
                lattice.cell().y * HEX_ROW_STEP / 2.0,
            ),
        );
    }

    #[test]
    fn cells_leave_a_gap() {
        for lattice in every_lattice() {
            let (_, inlay) = lattice.inlay(Coord::new(2, 2));
            assert!(inlay.x < lattice.cell().x && inlay.y < lattice.cell().y);
            // The gap is the same all round.
            let (anchor, _) = lattice.inlay(Coord::new(2, 2));
            let pulled = anchor - lattice.anchor(Coord::new(2, 2));
            close(pulled.x / lattice.cell().x, GAP);
            close(pulled.y / lattice.cell().y, GAP);
        }
    }

    #[test]
    fn fitting_picks_the_largest_size_that_fits() {
        for lattice in every_lattice() {
            let available = vec2(1000.0, 400.0);
            let size = lattice.fit(available, 0.0);
            let extent = lattice.extent(size);

            assert!(extent.x <= available.x + EPSILON);
            assert!(extent.y <= available.y + EPSILON);
            // A pixel less of room never fits a bigger cell.
            assert!(
                lattice.extent(size + 1.0).x > available.x
                    || lattice.extent(size + 1.0).y > available.y
            );
        }
    }

    #[test]
    fn a_minimum_size_wins_over_a_board_that_does_not_fit() {
        let lattice = Lattice::new(Grid::HEX);
        assert_eq!(lattice.fit(Vec2::splat(1.0), 12.0), 12.0);
    }

    #[test]
    fn decorations_stay_square() {
        assert_close(Lattice::new(Grid::SQUARE).square(50.0), vec2(50.0, 50.0));
        assert_close(
            Lattice::new(Grid::HEX).square(50.0),
            vec2(50.0, 50.0 * HEX_WIDTH_FOR_HEIGHT),
        );
    }
}
