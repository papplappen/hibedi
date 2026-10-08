//! What the board shows, derived from the game alone: the board's
//! entities only ever follow this description. Kept free of Bevy so the
//! rules for hints and highlights read (and test) on their own.

use hibedi_core::{Action, Cell, Coord, Game, Side, TokenId};

use crate::ui::keys;

/// Who looks at the board, and what they may do.
#[derive(Clone, Copy, Debug)]
pub struct Viewer {
    /// The side played from this screen online; `None` in hotseat, where
    /// the screen is shared.
    pub side: Option<Side>,
    /// Whether hints for the player to move show.
    pub hints: bool,
    /// Whether the viewer takes the actions now: they get keys.
    pub acts: bool,
}

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
    /// The key taking the action aimed at this cell.
    pub key: Option<char>,
}

/// What a click on a cell would do for the player to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    /// A new token may be placed here (or moved here, before it hops).
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
    /// Done for this turn: it jumped and may not be activated again.
    pub spent: bool,
    pub halo: Halo,
}

/// The ring around an activated token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halo {
    None,
    /// The player to move may jump with it (or switch to it, before the
    /// token chosen hops): it glows.
    Selectable,
    /// Activated, but waiting (or unable) to move.
    Pending,
}

impl BoardView {
    /// The board as `viewer` sees it.
    pub fn of(game: &Game, viewer: Viewer) -> Self {
        let board = game.board();
        let actions = game.actions();
        let aimed_at = |coord: Coord| {
            actions
                .iter()
                .find(|&&action| game.action_cell(action) == Some(coord))
        };
        let keyed: Vec<(char, Action)> = if viewer.acts {
            keys::label(&actions).collect()
        } else {
            Vec::new()
        };
        // The cells a chain may start from, or, before the token chosen
        // hops, the other choices it may be swapped for.
        let starts = game.start_options();
        let pending = game.pending_activated_tokens();

        let cells = (0..board.storage_height() as i32)
            .flat_map(|y| (0..board.storage_width() as i32).map(move |x| Coord::new(x, y)))
            .map(|coord| {
                let cell = board.cell_at(coord).unwrap_or(Cell::Empty);
                let hint = if !viewer.hints {
                    None
                } else if matches!(aimed_at(coord), Some(Action::Jump(_))) {
                    Some(Hint::Land)
                } else if starts.contains(&coord) && !board.is_occupied(coord) {
                    Some(Hint::Place)
                } else {
                    None
                };
                let key = keyed
                    .iter()
                    .find(|&&(_, action)| game.action_cell(action) == Some(coord))
                    .map(|&(key, _)| key);
                CellView {
                    coord,
                    cell,
                    used: game.is_crossed_this_chain(coord),
                    own: viewer.side.is_some() && cell.side() == viewer.side,
                    hint,
                    key,
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
                spent: game.is_spent(token.id),
                halo: if viewer.hints && starts.contains(&token.position) {
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

    const HOTSEAT: Viewer = Viewer {
        side: None,
        hints: true,
        acts: true,
    };

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

        let shown = BoardView::of(&game, HOTSEAT);
        assert_eq!(cell(&shown, 0, 1).hint, Some(Hint::Place));
        assert_eq!(cell(&shown, 1, 1).hint, None);

        let hidden = BoardView::of(
            &game,
            Viewer {
                hints: false,
                ..HOTSEAT
            },
        );
        assert!(hidden.cells.iter().all(|cell| cell.hint.is_none()));
    }

    #[test]
    fn a_jump_marks_its_landing_and_the_crossed_cells() {
        let mut game = Game::from_board(Board::from_rows(&["RR.R", "G..."]), Side::Red);
        assert!(game.apply(Action::Start(Coord::new(0, 1))));
        assert!(game.apply(Action::Jump(Direction::Right)));

        let view = BoardView::of(&game, HOTSEAT);
        assert!(cell(&view, 1, 1).used);
        assert!(cell(&view, 2, 1).used);
        assert_eq!(cell(&view, 5, 1).hint, Some(Hint::Land));
        assert_eq!(view.tokens.len(), 1);
        assert!(view.tokens[0].jumping);
    }

    #[test]
    fn only_an_online_viewer_owns_cells() {
        let game = Game::from_board(Board::from_rows(&["RG"]), Side::Red);

        let red = BoardView::of(
            &game,
            Viewer {
                side: Some(Side::Red),
                ..HOTSEAT
            },
        );
        assert!(cell(&red, 1, 1).own);
        assert!(!cell(&red, 2, 1).own);

        let hotseat = BoardView::of(&game, HOTSEAT);
        assert!(hotseat.cells.iter().all(|cell| !cell.own));
    }

    #[test]
    fn actions_are_keyed_row_by_row_for_the_player_acting() {
        let game = Game::from_board(Board::from_rows(&["R.", ".G"]), Side::Red);
        let keyed: Vec<(Coord, char)> = BoardView::of(&game, HOTSEAT)
            .cells
            .iter()
            .filter_map(|cell| Some((cell.coord, cell.key?)))
            .collect();
        assert_eq!(
            keyed,
            vec![
                (Coord::new(1, 0), '1'),
                (Coord::new(0, 1), '2'),
                (Coord::new(2, 1), '3'),
                (Coord::new(1, 2), '4'),
                (Coord::new(3, 2), '5'),
                (Coord::new(2, 3), '6'),
            ]
        );

        let watching = BoardView::of(
            &game,
            Viewer {
                acts: false,
                ..HOTSEAT
            },
        );
        assert!(watching.cells.iter().all(|cell| cell.key.is_none()));
    }

    #[test]
    fn hops_are_keyed_by_direction() {
        // Placed between the green cell and the red pair, the token may
        // hop either way.
        let mut game = Game::from_board(Board::from_rows(&["G.RR."]), Side::Red);
        assert!(game.apply(Action::Start(Coord::new(2, 1))));
        let view = BoardView::of(&game, HOTSEAT);
        assert_eq!(cell(&view, 0, 1).key, Some('a'));
        assert_eq!(cell(&view, 5, 1).key, Some('d'));
        assert!(game.actions().contains(&Action::Jump(Direction::Left)));
    }
}
