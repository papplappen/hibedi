use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;

use rand::{Rng, RngExt};

/// Default playable board size shared by client and server.
pub const BOARD_WIDTH: usize = 4;
pub const BOARD_HEIGHT: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Red,
    Green,
}

impl Side {
    /// Every side, in seat order: online, seat 0 plays red.
    pub const ALL: [Self; 2] = [Self::Red, Self::Green];

    pub const fn other(self) -> Self {
        match self {
            Self::Red => Self::Green,
            Self::Green => Self::Red,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Red => "Red",
            Self::Green => "Green",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cell {
    Empty,
    Red,
    Green,
}

impl Cell {
    pub const fn side(self) -> Option<Side> {
        match self {
            Self::Empty => None,
            Self::Red => Some(Side::Red),
            Self::Green => Some(Side::Green),
        }
    }

    pub const fn flipped(self) -> Self {
        match self {
            Self::Empty => Self::Empty,
            Self::Red => Self::Green,
            Self::Green => Self::Red,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
}

impl Coord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    const fn shifted(self, direction: Direction) -> Self {
        let (dx, dy) = direction.delta();
        Self::new(self.x + dx, self.y + dy)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Right,
    Down,
    Left,
}

impl Direction {
    pub const ALL: [Self; 4] = [Self::Up, Self::Right, Self::Down, Self::Left];

    pub const fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Right => (1, 0),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TokenId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Token {
    pub id: TokenId,
    pub position: Coord,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Jump {
    pub direction: Direction,
    pub crossed: Vec<Coord>,
    pub landing: Coord,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Board {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    tokens: Vec<Token>,
}

impl Board {
    pub fn random(width: usize, height: usize, rng: &mut impl Rng) -> Self {
        assert!(
            width.saturating_mul(height) >= 2,
            "the board needs at least two playable cells"
        );

        loop {
            let mut board = Self::empty(width, height);

            for y in 1..=height {
                for x in 1..=width {
                    let cell = match rng.random_range(0..4) {
                        0 | 1 => Cell::Empty,
                        2 => Cell::Red,
                        _ => Cell::Green,
                    };
                    board.set_cell(Coord::new(x as i32, y as i32), cell);
                }
            }

            if Side::ALL.iter().all(|&side| board.colored_count(side) > 0) {
                return board;
            }
        }
    }

    fn empty(width: usize, height: usize) -> Self {
        let storage_width = width + 2;
        let storage_height = height + 2;

        Self {
            width,
            height,
            cells: vec![Cell::Empty; storage_width * storage_height],
            tokens: Vec::new(),
        }
    }

    /// Builds a board from row strings using `.` for empty, `R` for red and
    /// `G` for green. The outer ring is left empty. Intended for tests and
    /// scenario setup.
    pub fn from_rows(rows: &[&str]) -> Self {
        let height = rows.len();
        let width = rows.first().map_or(0, |row| row.len());
        let mut board = Self::empty(width, height);

        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), width, "all board rows must have equal length");
            for (x, character) in row.chars().enumerate() {
                let cell = match character {
                    '.' => Cell::Empty,
                    'R' => Cell::Red,
                    'G' => Cell::Green,
                    other => panic!("unexpected board character: {other}"),
                };
                board.set_cell(Coord::new((x + 1) as i32, (y + 1) as i32), cell);
            }
        }

        board
    }

    pub const fn storage_width(&self) -> usize {
        self.width + 2
    }

    pub const fn storage_height(&self) -> usize {
        self.height + 2
    }

    pub fn is_in_storage(&self, coord: Coord) -> bool {
        coord.x >= 0
            && coord.y >= 0
            && (coord.x as usize) < self.storage_width()
            && (coord.y as usize) < self.storage_height()
    }

    pub fn is_playable_cell(&self, coord: Coord) -> bool {
        coord.x >= 1
            && coord.y >= 1
            && (coord.x as usize) <= self.width
            && (coord.y as usize) <= self.height
    }

    pub fn cell_at(&self, coord: Coord) -> Option<Cell> {
        self.is_in_storage(coord)
            .then(|| self.cells[self.index(coord)])
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn token_at(&self, coord: Coord) -> Option<TokenId> {
        self.tokens
            .iter()
            .find(|token| token.position == coord)
            .map(|token| token.id)
    }

    pub fn token_position(&self, id: TokenId) -> Option<Coord> {
        self.tokens
            .iter()
            .find(|token| token.id == id)
            .map(|token| token.position)
    }

    pub fn is_occupied(&self, coord: Coord) -> bool {
        self.token_at(coord).is_some()
    }

    /// Number of playable cells colored `side`.
    pub fn colored_count(&self, side: Side) -> usize {
        self.playable_coords()
            .filter(|&coord| self.cells[self.index(coord)].side() == Some(side))
            .count()
    }

    /// The side holding more colored cells; `None` on equal counts.
    pub fn majority(&self) -> Option<Side> {
        match self
            .colored_count(Side::Red)
            .cmp(&self.colored_count(Side::Green))
        {
            Ordering::Greater => Some(Side::Red),
            Ordering::Less => Some(Side::Green),
            Ordering::Equal => None,
        }
    }

    pub fn legal_initial_placements(&self) -> Vec<Coord> {
        self.initial_placements().collect()
    }

    pub fn has_initial_move(&self) -> bool {
        self.initial_placements().next().is_some()
    }

    /// Cells a new token may be placed on: any free empty cell (outer
    /// ring included) with a jump available from it.
    fn initial_placements(&self) -> impl Iterator<Item = Coord> {
        let no_history = BTreeSet::new();
        self.storage_coords().filter(move |&coord| {
            self.can_host_token(coord)
                && Direction::ALL.into_iter().any(|direction| {
                    self.legal_jump_from(coord, direction, &no_history)
                        .is_some()
                })
        })
    }

    /// Every cell inside the outer ring, row by row.
    fn playable_coords(&self) -> impl Iterator<Item = Coord> {
        self.storage_coords()
            .filter(|&coord| self.is_playable_cell(coord))
    }

    /// Every cell of the storage grid, row by row.
    fn storage_coords(&self) -> impl Iterator<Item = Coord> {
        let width = self.storage_width() as i32;
        let height = self.storage_height() as i32;
        (0..height).flat_map(move |y| (0..width).map(move |x| Coord::new(x, y)))
    }

    pub fn legal_jump_from(
        &self,
        origin: Coord,
        direction: Direction,
        crossed_this_chain: &BTreeSet<Coord>,
    ) -> Option<Jump> {
        if !self.is_in_storage(origin) {
            return None;
        }

        let mut cursor = origin.shifted(direction);
        let mut crossed = Vec::new();

        while self.is_in_storage(cursor) {
            let Some(cell) = self.cell_at(cursor) else {
                break;
            };

            if cell.side().is_none() {
                break;
            }

            if crossed_this_chain.contains(&cursor) {
                return None;
            }

            crossed.push(cursor);
            cursor = cursor.shifted(direction);
        }

        if crossed.is_empty()
            || !self.is_in_storage(cursor)
            || self.cell_at(cursor) != Some(Cell::Empty)
            || self.is_occupied(cursor)
        {
            return None;
        }

        Some(Jump {
            direction,
            crossed,
            landing: cursor,
        })
    }

    pub fn legal_jumps_for_token(
        &self,
        id: TokenId,
        crossed_this_chain: &BTreeSet<Coord>,
    ) -> Vec<Jump> {
        let Some(origin) = self.token_position(id) else {
            return Vec::new();
        };

        Direction::ALL
            .into_iter()
            .filter_map(|direction| self.legal_jump_from(origin, direction, crossed_this_chain))
            .collect()
    }

    fn index(&self, coord: Coord) -> usize {
        debug_assert!(self.is_in_storage(coord));
        coord.y as usize * self.storage_width() + coord.x as usize
    }

    fn set_cell(&mut self, coord: Coord, cell: Cell) {
        debug_assert!(self.is_in_storage(coord));
        debug_assert!(self.is_playable_cell(coord) || cell == Cell::Empty);
        let index = self.index(coord);
        self.cells[index] = cell;
    }

    fn can_host_token(&self, coord: Coord) -> bool {
        self.is_in_storage(coord)
            && self.cell_at(coord) == Some(Cell::Empty)
            && !self.is_occupied(coord)
    }

    /// Adds a token when the cell can host one. Intended for tests and
    /// scenario setup; gameplay places with `Action::Place`.
    pub fn add_token(&mut self, id: TokenId, position: Coord) -> bool {
        if !self.can_host_token(position) {
            return false;
        }

        self.tokens.push(Token { id, position });
        true
    }

    fn move_token(&mut self, id: TokenId, landing: Coord) -> bool {
        if !self.can_host_token(landing) {
            return false;
        }

        let Some(token) = self.tokens.iter_mut().find(|token| token.id == id) else {
            return false;
        };
        token.position = landing;
        true
    }

    fn flip_cells(&mut self, crossed: &[Coord]) {
        for &coord in crossed {
            let Some(cell) = self.cell_at(coord) else {
                continue;
            };
            self.set_cell(coord, cell.flipped());
        }
    }

    /// The side owning every colored cell, if one does.
    fn monochrome_side(&self) -> Option<Side> {
        Side::ALL
            .into_iter()
            .find(|&side| self.colored_count(side) > 0 && self.colored_count(side.other()) == 0)
    }

    fn majority_result(&self) -> GameResult {
        self.majority()
            .map_or(GameResult::Draw, |side| GameResult::Winner {
                side,
                reason: WinReason::Majority,
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    PlaceInitialToken,
    ChooseJump,
    ChooseActivatedToken,
    GameOver,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WinReason {
    Monochrome,
    Majority,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameResult {
    Winner { side: Side, reason: WinReason },
    Draw,
}

/// One decision of the player to move. Everything a player can do is an
/// action, and [`Game::actions`] lists every one available: a player,
/// human or bot, picks one of them and [`Game::apply`]s it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "at", rename_all = "snake_case")]
pub enum Action {
    /// Places a new token on a cell; it starts jumping right away.
    Place(Coord),
    /// Hops the jumping token in a direction.
    Jump(Direction),
    /// Chooses the activated token to jump with next.
    Select(TokenId),
    /// Takes back the choice of the activated token, before its first
    /// hop: the choice only locks once jumping starts.
    Unselect,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Game {
    board: Board,
    current_player: Side,
    phase: Phase,
    current_token: Option<TokenId>,
    crossed_this_chain: BTreeSet<Coord>,
    turn_start_tokens: BTreeSet<TokenId>,
    activated_this_turn: BTreeSet<TokenId>,
    pending_activated: Vec<TokenId>,
    next_token_id: u32,
    result: Option<GameResult>,
}

impl Game {
    pub fn new_random(width: usize, height: usize, rng: &mut impl Rng) -> Self {
        let board = Board::random(width, height, rng);
        let tie_breaker = if rng.random_range(0..2) == 0 {
            Side::Red
        } else {
            Side::Green
        };
        Self::from_board(board, tie_breaker)
    }

    /// Starts a game from a prepared board. The first player is the side
    /// with fewer colored cells; `tie_breaker` decides equal counts.
    pub fn from_board(board: Board, tie_breaker: Side) -> Self {
        let current_player = board.majority().map_or(tie_breaker, Side::other);
        let next_token_id = board
            .tokens
            .iter()
            .map(|token| token.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);

        let mut game = Self {
            board,
            current_player,
            phase: Phase::PlaceInitialToken,
            current_token: None,
            crossed_this_chain: BTreeSet::new(),
            turn_start_tokens: BTreeSet::new(),
            activated_this_turn: BTreeSet::new(),
            pending_activated: Vec::new(),
            next_token_id,
            result: None,
        };
        game.begin_turn_or_finish();
        game
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub const fn current_player(&self) -> Side {
        self.current_player
    }

    pub const fn phase(&self) -> Phase {
        self.phase
    }

    pub const fn current_token(&self) -> Option<TokenId> {
        self.current_token
    }

    pub fn is_crossed_this_chain(&self, coord: Coord) -> bool {
        self.crossed_this_chain.contains(&coord)
    }

    pub const fn result(&self) -> Option<GameResult> {
        self.result
    }

    /// Every action the player to move may take, ordered by the cell it
    /// is aimed at, row by row (see [`Game::action_cell`]); `Unselect`,
    /// aimed at no cell, comes last. Empty once the game is over.
    pub fn actions(&self) -> Vec<Action> {
        let mut actions: Vec<Action> = match self.phase {
            Phase::PlaceInitialToken => self
                .board
                .legal_initial_placements()
                .into_iter()
                .map(Action::Place)
                .collect(),
            Phase::ChooseJump => self
                .legal_current_jumps()
                .into_iter()
                .map(|jump| Action::Jump(jump.direction))
                .chain(self.can_unselect().then_some(Action::Unselect))
                .collect(),
            Phase::ChooseActivatedToken => self
                .movable_pending_tokens()
                .into_iter()
                .map(Action::Select)
                .collect(),
            Phase::GameOver => Vec::new(),
        };
        actions.sort_by_key(|&action| {
            self.action_cell(action)
                .map_or((1, 0, 0), |cell| (0, cell.y, cell.x))
        });
        actions
    }

    /// The cell an action is aimed at: where a token is placed or lands,
    /// or the token selected. `None` for `Unselect`, and for an action
    /// that is not available.
    pub fn action_cell(&self, action: Action) -> Option<Coord> {
        match action {
            Action::Place(coord) => Some(coord),
            Action::Jump(direction) => self
                .legal_current_jumps()
                .into_iter()
                .find(|jump| jump.direction == direction)
                .map(|jump| jump.landing),
            Action::Select(token) => self.board.token_position(token),
            Action::Unselect => None,
        }
    }

    /// Takes an action of the player to move. Exactly the actions listed
    /// by [`Game::actions`] are accepted; anything else changes nothing
    /// and returns false.
    #[must_use = "the action is not taken when it returns false"]
    pub fn apply(&mut self, action: Action) -> bool {
        match action {
            Action::Place(coord) => self.place_initial_token(coord),
            Action::Jump(direction) => self.jump(direction),
            Action::Select(token) => self.select_activated_token(token),
            Action::Unselect => self.unselect(),
        }
    }

    /// The jumps open to the jumping token.
    pub fn legal_current_jumps(&self) -> Vec<Jump> {
        if self.phase != Phase::ChooseJump {
            return Vec::new();
        }

        self.current_token
            .map(|token| {
                self.board
                    .legal_jumps_for_token(token, &self.crossed_this_chain)
            })
            .unwrap_or_default()
    }

    pub fn pending_activated_tokens(&self) -> &[TokenId] {
        &self.pending_activated
    }

    /// The activated tokens the player to move may jump with: the ones
    /// to choose from, or, before the first hop of a chosen one, the
    /// others it may still be swapped for (`Unselect`, then `Select`).
    pub fn selectable_activated_tokens(&self) -> Vec<TokenId> {
        match self.phase {
            Phase::ChooseActivatedToken => self.movable_pending_tokens(),
            Phase::ChooseJump if self.crossed_this_chain.is_empty() => {
                self.movable_pending_tokens()
            }
            _ => Vec::new(),
        }
    }

    /// Whether a token is done for this turn: it has jumped, and is
    /// neither jumping now nor able to be activated again.
    pub fn is_spent(&self, token: TokenId) -> bool {
        matches!(self.phase, Phase::ChooseJump | Phase::ChooseActivatedToken)
            && self.current_token != Some(token)
            && !self.pending_activated.contains(&token)
            && (!self.turn_start_tokens.contains(&token)
                || self.activated_this_turn.contains(&token))
    }

    fn place_initial_token(&mut self, coord: Coord) -> bool {
        if self.phase != Phase::PlaceInitialToken
            || !self.board.legal_initial_placements().contains(&coord)
        {
            return false;
        }

        let token = TokenId(self.next_token_id);
        self.next_token_id = self.next_token_id.saturating_add(1);

        if !self.board.add_token(token, coord) {
            return false;
        }

        self.current_token = Some(token);
        self.crossed_this_chain.clear();
        self.phase = Phase::ChooseJump;
        true
    }

    fn jump(&mut self, direction: Direction) -> bool {
        if self.phase != Phase::ChooseJump {
            return false;
        }

        let Some(token) = self.current_token else {
            return false;
        };
        let Some(origin) = self.board.token_position(token) else {
            return false;
        };
        let Some(jump) = self
            .board
            .legal_jump_from(origin, direction, &self.crossed_this_chain)
        else {
            return false;
        };

        if !self.board.move_token(token, jump.landing) {
            return false;
        }

        self.board.flip_cells(&jump.crossed);
        self.activate_neighbors(&jump.crossed);

        if let Some(side) = self.board.monochrome_side() {
            self.finish_with_result(GameResult::Winner {
                side,
                reason: WinReason::Monochrome,
            });
            return true;
        }

        self.crossed_this_chain.extend(jump.crossed);

        if self
            .board
            .legal_jumps_for_token(token, &self.crossed_this_chain)
            .is_empty()
        {
            self.finish_current_chain();
        }

        true
    }

    fn select_activated_token(&mut self, token: TokenId) -> bool {
        if self.phase != Phase::ChooseActivatedToken
            || !self.movable_pending_tokens().contains(&token)
        {
            return false;
        }

        self.pending_activated.retain(|pending| *pending != token);
        self.current_token = Some(token);
        self.crossed_this_chain.clear();
        self.phase = Phase::ChooseJump;
        true
    }

    /// Whether the jumping token was chosen among others and has not
    /// hopped yet. A freshly placed token was never chosen: nothing is
    /// pending before its first hop.
    fn can_unselect(&self) -> bool {
        self.phase == Phase::ChooseJump
            && self.crossed_this_chain.is_empty()
            && !self.movable_pending_tokens().is_empty()
    }

    /// Returns the chosen token to the pending ones, so the turn asks
    /// for a token again.
    fn unselect(&mut self) -> bool {
        if !self.can_unselect() {
            return false;
        }
        let Some(token) = self.current_token.take() else {
            return false;
        };
        self.pending_activated.push(token);
        self.phase = Phase::ChooseActivatedToken;
        true
    }

    fn begin_turn_or_finish(&mut self) {
        self.current_token = None;
        self.crossed_this_chain.clear();
        self.activated_this_turn.clear();
        self.pending_activated.clear();
        self.turn_start_tokens = self.board.tokens.iter().map(|token| token.id).collect();

        if self.board.has_initial_move() {
            self.phase = Phase::PlaceInitialToken;
            self.result = None;
        } else {
            self.finish_with_result(self.board.majority_result());
        }
    }

    fn activate_neighbors(&mut self, crossed: &[Coord]) {
        for &coord in crossed {
            for direction in Direction::ALL {
                let neighbor = coord.shifted(direction);
                let Some(token) = self.board.token_at(neighbor) else {
                    continue;
                };

                if self.turn_start_tokens.contains(&token) && self.activated_this_turn.insert(token)
                {
                    self.pending_activated.push(token);
                }
            }
        }
    }

    fn movable_pending_tokens(&self) -> Vec<TokenId> {
        let no_history = BTreeSet::new();
        self.pending_activated
            .iter()
            .copied()
            .filter(|token| {
                !self
                    .board
                    .legal_jumps_for_token(*token, &no_history)
                    .is_empty()
            })
            .collect()
    }

    fn finish_current_chain(&mut self) {
        self.current_token = None;
        self.crossed_this_chain.clear();

        if self.movable_pending_tokens().is_empty() {
            self.current_player = self.current_player.other();
            self.begin_turn_or_finish();
        } else {
            self.phase = Phase::ChooseActivatedToken;
        }
    }

    fn finish_with_result(&mut self, result: GameResult) {
        self.current_token = None;
        self.crossed_this_chain.clear();
        self.phase = Phase::GameOver;
        self.result = Some(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_from_rows(rows: &[&str]) -> Board {
        Board::from_rows(rows)
    }

    #[test]
    fn edge_run_can_land_in_the_outer_ring() {
        let mut board = board_from_rows(&["RRR"]);
        let token = TokenId(1);
        assert!(board.add_token(token, Coord::new(0, 1)));

        let jump = board
            .legal_jumps_for_token(token, &BTreeSet::new())
            .into_iter()
            .find(|jump| jump.direction == Direction::Right)
            .expect("the edge run should be jumpable");

        assert_eq!(
            jump.crossed,
            vec![Coord::new(1, 1), Coord::new(2, 1), Coord::new(3, 1)]
        );
        assert_eq!(jump.landing, Coord::new(4, 1));
    }

    #[test]
    fn outer_ring_can_be_an_initial_placement() {
        let board = board_from_rows(&["R.."]);

        assert!(board.legal_initial_placements().contains(&Coord::new(0, 1)));
    }

    #[test]
    fn occupied_landing_blocks_a_jump() {
        let mut board = board_from_rows(&["RRR"]);
        assert!(board.add_token(TokenId(1), Coord::new(0, 1)));
        assert!(board.add_token(TokenId(2), Coord::new(4, 1)));

        assert!(
            board
                .legal_jumps_for_token(TokenId(1), &BTreeSet::new())
                .is_empty()
        );
    }

    #[test]
    fn crossed_cells_are_only_forbidden_within_one_chain() {
        let mut board = board_from_rows(&["RRR"]);
        let token = TokenId(1);
        assert!(board.add_token(token, Coord::new(0, 1)));

        let mut history = BTreeSet::new();
        history.insert(Coord::new(2, 1));

        assert!(board.legal_jumps_for_token(token, &history).is_empty());
        assert!(
            !board
                .legal_jumps_for_token(token, &BTreeSet::new())
                .is_empty()
        );
    }

    #[test]
    fn chosen_token_can_be_taken_back_before_the_first_hop() {
        let mut board = board_from_rows(&[".......", ".RR....", ".RR.R..", ".......", "GG....."]);
        let first = TokenId(10);
        let second = TokenId(11);
        assert!(board.add_token(first, Coord::new(1, 2)));
        assert!(board.add_token(second, Coord::new(1, 3)));
        let mut game = Game::from_board(board, Side::Red);

        // Green (fewer cells) places above the red column and jumps
        // through it (two compulsory hops), activating both waiting
        // tokens.
        assert!(game.apply(Action::Place(Coord::new(2, 1))));
        // A freshly placed token was not chosen among others.
        assert!(!game.actions().contains(&Action::Unselect));
        assert!(game.apply(Action::Jump(Direction::Down)));
        assert!(game.apply(Action::Jump(Direction::Down)));

        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert_eq!(
            game.actions(),
            vec![Action::Select(first), Action::Select(second)]
        );

        // Choosing one does not lock the choice in yet: the other one
        // still glows, and the choice can be taken back.
        assert!(game.apply(Action::Select(first)));
        assert_eq!(game.current_token(), Some(first));
        assert_eq!(game.phase(), Phase::ChooseJump);
        assert_eq!(game.selectable_activated_tokens(), vec![second]);
        assert_eq!(
            game.actions(),
            vec![Action::Jump(Direction::Right), Action::Unselect]
        );
        assert!(!game.apply(Action::Select(second)));

        assert!(game.apply(Action::Unselect));
        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert_eq!(game.current_token(), None);
        assert!(game.apply(Action::Select(second)));
        assert!(game.pending_activated_tokens().contains(&first));

        // Once the chain has started hopping, the choice locks.
        assert!(game.apply(Action::Jump(Direction::Right)));
        assert_eq!(game.phase(), Phase::ChooseJump);
        assert!(game.selectable_activated_tokens().is_empty());
        assert!(!game.apply(Action::Unselect));

        // The chain runs out of jumps after the second hop, and the
        // remaining activated token is asked for as usual.
        assert!(game.apply(Action::Jump(Direction::Right)));
        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert_eq!(game.actions(), vec![Action::Select(first)]);
    }

    #[test]
    fn tokens_that_jumped_are_spent_for_the_turn() {
        let mut board = board_from_rows(&[".......", ".RR....", ".RR.R..", ".......", "GG....."]);
        let first = TokenId(10);
        let second = TokenId(11);
        assert!(board.add_token(first, Coord::new(1, 2)));
        assert!(board.add_token(second, Coord::new(1, 3)));
        let mut game = Game::from_board(board, Side::Red);
        assert!(!game.is_spent(first));

        assert!(game.apply(Action::Place(Coord::new(2, 1))));
        let placed = game.current_token().unwrap();
        assert!(game.apply(Action::Jump(Direction::Down)));
        // Jumping is not spent yet.
        assert!(!game.is_spent(placed));
        assert!(game.apply(Action::Jump(Direction::Down)));
        assert!(game.is_spent(placed));
        // Activated, waiting to jump.
        assert!(!game.is_spent(first));
        assert!(!game.is_spent(second));

        assert!(game.apply(Action::Select(second)));
        assert!(game.apply(Action::Jump(Direction::Right)));
        assert!(game.apply(Action::Jump(Direction::Right)));
        assert!(game.is_spent(second));
        assert!(!game.is_spent(first));
    }

    #[test]
    fn exactly_the_listed_actions_apply() {
        let mut rng = rand::rng();
        for _ in 0..200 {
            let mut game = Game::new_random(BOARD_WIDTH, BOARD_HEIGHT, &mut rng);
            // Every turn places a token, so games are short.
            for _ in 0..1000 {
                let actions = game.actions();
                if game.phase() == Phase::GameOver {
                    assert!(actions.is_empty());
                    break;
                }
                assert!(!actions.is_empty(), "a running game offers actions");
                for &action in &actions {
                    assert!(
                        game.clone().apply(action),
                        "{action:?} is listed but not applicable"
                    );
                }
                for direction in Direction::ALL {
                    let action = Action::Jump(direction);
                    assert_eq!(game.clone().apply(action), actions.contains(&action));
                }
                let action = actions[rng.random_range(0..actions.len())];
                assert!(game.apply(action));
            }
            assert_eq!(game.phase(), Phase::GameOver);
        }
    }

    #[test]
    fn crossing_activates_cardinal_old_tokens_once() {
        let mut board = board_from_rows(&["...", ".R.", "R.."]);
        let cardinal = TokenId(10);
        let diagonal = TokenId(11);
        assert!(board.add_token(cardinal, Coord::new(2, 1)));
        assert!(board.add_token(diagonal, Coord::new(1, 1)));
        let mut game = Game::from_board(board, Side::Red);

        assert!(game.apply(Action::Place(Coord::new(1, 2))));
        assert!(game.apply(Action::Jump(Direction::Right)));

        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert_eq!(game.pending_activated_tokens(), &[cardinal]);
        assert_eq!(game.selectable_activated_tokens(), vec![cardinal]);
    }

    #[test]
    fn matching_color_wins_even_when_the_other_player_made_the_jump() {
        let board = board_from_rows(&["R.G"]);
        let mut game = Game::from_board(board, Side::Green);

        assert_eq!(game.current_player(), Side::Green);
        assert!(game.apply(Action::Place(Coord::new(4, 1))));
        assert!(game.apply(Action::Jump(Direction::Left)));

        assert_eq!(game.phase(), Phase::GameOver);
        assert_eq!(
            game.result(),
            Some(GameResult::Winner {
                side: Side::Red,
                reason: WinReason::Monochrome,
            })
        );
    }

    #[test]
    fn current_token_must_continue_when_a_jump_is_available() {
        let board = board_from_rows(&["RR.R", "G..."]);
        let mut game = Game::from_board(board, Side::Red);

        assert!(game.apply(Action::Place(Coord::new(0, 1))));
        assert!(game.apply(Action::Jump(Direction::Right)));

        assert_eq!(game.phase(), Phase::ChooseJump);
        assert_eq!(
            game.board().token_position(game.current_token().unwrap()),
            Some(Coord::new(3, 1))
        );
        assert!(
            game.legal_current_jumps()
                .iter()
                .any(|jump| jump.direction == Direction::Right)
        );
    }

    #[test]
    fn outer_ring_tokens_can_be_activated() {
        let mut board = board_from_rows(&[".R.", "R.."]);
        let outer_token = TokenId(12);
        assert!(board.add_token(outer_token, Coord::new(2, 0)));
        let mut game = Game::from_board(board, Side::Red);

        assert!(game.apply(Action::Place(Coord::new(1, 1))));
        assert!(game.apply(Action::Jump(Direction::Right)));

        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert_eq!(game.pending_activated_tokens(), &[outer_token]);
    }

    #[test]
    fn games_round_trip_through_json_deterministically() {
        // The game state is the document players share online: it must
        // survive serialization and serialize the same way every time.
        let mut board = board_from_rows(&["RR.R", "G..."]);
        assert!(board.add_token(TokenId(7), Coord::new(5, 2)));
        let mut game = Game::from_board(board, Side::Red);
        assert!(game.apply(Action::Place(Coord::new(0, 1))));
        assert!(game.apply(Action::Jump(Direction::Right)));

        let json = serde_json::to_string(&game).unwrap();
        let back: Game = serde_json::from_str(&json).unwrap();
        assert_eq!(back, game);
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }

    #[test]
    fn player_with_fewer_initial_cells_starts() {
        let board = board_from_rows(&["RR", "RG"]);
        let game = Game::from_board(board, Side::Red);

        assert_eq!(game.current_player(), Side::Green);
    }

    #[test]
    fn dead_board_uses_colored_cell_majority() {
        let mut board = board_from_rows(&["RR", "RG"]);
        let mut id = 1;
        for y in 0..board.storage_height() as i32 {
            for x in 0..board.storage_width() as i32 {
                let coord = Coord::new(x, y);
                if !board.is_playable_cell(coord) {
                    assert!(board.add_token(TokenId(id), coord));
                    id += 1;
                }
            }
        }

        let game = Game::from_board(board, Side::Red);
        assert_eq!(game.phase(), Phase::GameOver);
        assert_eq!(
            game.result(),
            Some(GameResult::Winner {
                side: Side::Red,
                reason: WinReason::Majority,
            })
        );
    }
}
