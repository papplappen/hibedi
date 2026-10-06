//! What the board shows, derived from the game alone: the board's
//! entities only ever follow this description. Kept free of Bevy so the
//! rules for hints and highlights read (and test) on their own.

use hibedi_core::{Cell, Coord, Game, Side, TokenId};

pub struct BoardView {
    pub cells: Vec<CellView>,
    pub tokens: Vec<TokenView>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellView {
    pub coord: Coord,
    pub cell: Cell,
    /// Crossed by the running chain: no jump may cross it again.
    pub used: bool,
    /// Colored in the viewer's own side (online only).
    pub own: bool,
    pub hint: Option<Hint>,
}

/// What a click on a cell would do for the player to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    /// A new token may be placed here.
    Place,
    /// The jumping token may land here.
    Land,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TokenView {
    pub id: TokenId,
    pub position: Coord,
    /// The token currently jumping.
    pub jumping: bool,
    pub halo: Halo,
}

/// The ring around an activated token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halo {
    None,
    /// The player to move may jump with it: it glows.
    Selectable,
    /// Activated, but waiting (or unable) to move.
    Pending,
}

impl BoardView {
    /// The board as `viewer` sees it (`None` in hotseat, where the
    /// screen is shared). Hints show only when `hints` is set.
    pub fn of(game: &Game, hints: bool, viewer: Option<Side>) -> Self {
        let board = game.board();
        let placements = game.legal_initial_placements();
        let landings: Vec<Coord> = game
            .legal_current_jumps()
            .into_iter()
            .map(|jump| jump.landing)
            .collect();
        let selectable = game.selectable_activated_tokens();
        let pending = game.pending_activated_tokens();

        let cells = (0..board.storage_height() as i32)
            .flat_map(|y| (0..board.storage_width() as i32).map(move |x| Coord::new(x, y)))
            .map(|coord| {
                let cell = board.cell_at(coord).unwrap_or(Cell::Empty);
                let hint = if !hints {
                    None
                } else if landings.contains(&coord) {
                    Some(Hint::Land)
                } else if placements.contains(&coord) {
                    Some(Hint::Place)
                } else {
                    None
                };
                CellView {
                    coord,
                    cell,
                    used: game.is_crossed_this_chain(coord),
                    own: viewer.is_some() && cell.side() == viewer,
                    hint,
                }
            })
            .collect();

        let tokens = board
            .tokens()
            .iter()
            .map(|token| TokenView {
                id: token.id,
                position: token.position,
                jumping: game.current_token() == Some(token.id),
                halo: if hints && selectable.contains(&token.id) {
                    Halo::Selectable
                } else if pending.contains(&token.id) {
                    Halo::Pending
                } else {
                    Halo::None
                },
            })
            .collect();

        Self { cells, tokens }
    }
}

#[cfg(test)]
mod tests {
    use hibedi_core::{Board, Direction};

    use super::*;

    fn cell(view: &BoardView, x: i32, y: i32) -> CellView {
        *view
            .cells
            .iter()
            .find(|cell| cell.coord == Coord::new(x, y))
            .expect("the cell is on the board")
    }

    #[test]
    fn placement_cells_are_hinted_only_with_hints() {
        let game = Game::from_board(Board::from_rows(&["R.."]), Side::Red);

        let shown = BoardView::of(&game, true, None);
        assert_eq!(cell(&shown, 0, 1).hint, Some(Hint::Place));
        assert_eq!(cell(&shown, 1, 1).hint, None);

        let hidden = BoardView::of(&game, false, None);
        assert!(hidden.cells.iter().all(|cell| cell.hint.is_none()));
    }

    #[test]
    fn a_jump_marks_its_landing_and_the_crossed_cells() {
        let mut game = Game::from_board(Board::from_rows(&["RR.R", "G..."]), Side::Red);
        assert!(game.try_place_initial_token(Coord::new(0, 1)));
        assert!(game.try_jump(Direction::Right));

        let view = BoardView::of(&game, true, None);
        assert!(cell(&view, 1, 1).used);
        assert!(cell(&view, 2, 1).used);
        assert_eq!(cell(&view, 5, 1).hint, Some(Hint::Land));
        assert_eq!(view.tokens.len(), 1);
        assert!(view.tokens[0].jumping);
    }

    #[test]
    fn only_an_online_viewer_owns_cells() {
        let game = Game::from_board(Board::from_rows(&["RG"]), Side::Red);

        let red = BoardView::of(&game, false, Some(Side::Red));
        assert!(cell(&red, 1, 1).own);
        assert!(!cell(&red, 2, 1).own);

        let hotseat = BoardView::of(&game, false, None);
        assert!(hotseat.cells.iter().all(|cell| !cell.own));
    }
}
