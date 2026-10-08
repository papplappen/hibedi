use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;

use rand::{Rng, RngExt};

/// The cells a board is played on: a square grid, or a rhombus of hexes.
/// A grid owns what a coordinate means — which cells exist, which are
/// playable, and which cells neighbour each other — so the rules never
/// assume a shape.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Grid {
    /// Cells on rows and columns, stepping up, right, down and left.
    Square { width: usize, height: usize },
    /// A rhombus of hexes: no cell sits straight above another, and
    /// rows shift half a cell sideways.
    Hex { columns: usize, rows: usize },
}

impl Grid {
    /// The board dealt by default, and the square grid to compare hexes
    /// against.
    pub const SQUARE: Self = Self::Square {
        width: 4,
        height: 5,
    };

    /// The same number of cells as [`Grid::SQUARE`], on a hex lattice.
    pub const HEX: Self = Self::Hex {
        columns: 4,
        rows: 5,
    };

    pub const fn is_hex(self) -> bool {
        matches!(self, Self::Hex { .. })
    }

    /// Playable cells, not counting the outer ring.
    pub const fn playable_count(self) -> usize {
        match self {
            Self::Square { width, height } => width * height,
            Self::Hex { columns, rows } => columns * rows,
        }
    }

    /// How many cells wide and high the grid is, its outer ring
    /// included.
    pub const fn storage_size(self) -> (usize, usize) {
        let (width, height) = self.playable_size();
        (width + 2, height + 2)
    }

    /// How many playable cells wide and high the grid is.
    pub const fn playable_size(self) -> (usize, usize) {
        match self {
            Self::Square { width, height } => (width, height),
            Self::Hex { columns, rows } => (columns, rows),
        }
    }

    /// The directions a jump or an activation may take on this grid.
    pub const fn directions(self) -> &'static [Direction] {
        match self {
            Self::Square { .. } => &[
                Direction::Up,
                Direction::Right,
                Direction::Down,
                Direction::Left,
            ],
            Self::Hex { .. } => &[
                Direction::Right,
                Direction::DownRight,
                Direction::DownLeft,
                Direction::Left,
                Direction::UpLeft,
                Direction::UpRight,
            ],
        }
    }

    /// The step from a cell to its neighbour in `direction`. Only the
    /// directions [`Grid::directions`] lists are ever asked for; the
    /// others are the plain diagonal steps, so the answer is still a
    /// step rather than a hole in the match.
    pub const fn step(self, direction: Direction) -> Coord {
        let (dx, dy) = match (self.is_hex(), direction) {
            (false, Direction::Up) => (0, -1),
            (false, Direction::Down) => (0, 1),
            (false, Direction::Left) => (-1, 0),
            (false, Direction::Right) => (1, 0),
            (false, Direction::UpLeft) => (-1, -1),
            (false, Direction::DownLeft) => (-1, 1),
            (false, Direction::UpRight) => (1, -1),
            (false, Direction::DownRight) => (1, 1),
            (true, Direction::Left) => (-1, 0),
            (true, Direction::Right) => (1, 0),
            (true, Direction::UpLeft) => (0, -1),
            (true, Direction::DownRight) => (0, 1),
            (true, Direction::DownLeft) => (-1, 1),
            (true, Direction::UpRight) => (1, -1),
            // Up and down are not axes of a hex lattice: its cells sit
            // half a step off to the side instead.
            (true, Direction::Up | Direction::Down) => (0, -1),
        };
        Coord::new(dx, dy)
    }

    /// Whether `coord` is a playable cell, as opposed to the outer ring.
    pub const fn contains_playable(self, coord: Coord) -> bool {
        let (width, height) = self.playable_size();
        coord.x >= 1 && coord.y >= 1 && (coord.x as usize) <= width && (coord.y as usize) <= height
    }

    /// Whether `coord` is any cell the grid holds, the ring included.
    pub const fn contains_storage(self, coord: Coord) -> bool {
        let (width, height) = self.storage_size();
        let in_box = coord.x >= 0
            && coord.y >= 0
            && (coord.x as usize) < width
            && (coord.y as usize) < height;

        match self {
            // Every cell of the box is playable or a step from one.
            Self::Square { .. } => in_box,
            Self::Hex { columns, rows } => {
                // The box's far corners are a step from the ring only:
                // no run of colored cells reaches past the playable
                // cells to them, so they are not cells.
                let ring_corner = (coord.x == 0 && coord.y == 0)
                    || ((coord.x as usize) == columns + 1 && (coord.y as usize) == rows + 1);
                in_box && !ring_corner
            }
        }
    }

    /// Every playable cell, row by row.
    pub fn playable_coords(self) -> impl Iterator<Item = Coord> {
        let (width, height) = self.storage_size();
        (0..height as i32)
            .flat_map(move |y| (0..width as i32).map(move |x| Coord::new(x, y)))
            .filter(move |&coord| self.contains_playable(coord))
    }

    /// Every cell of the storage grid, row by row.
    pub fn storage_coords(self) -> impl Iterator<Item = Coord> {
        let (width, height) = self.storage_size();
        (0..height as i32)
            .flat_map(move |y| (0..width as i32).map(move |x| Coord::new(x, y)))
            .filter(move |&coord| self.contains_storage(coord))
    }

    /// The grid a toggle between the two switches to.
    pub const fn other(self) -> Self {
        match self {
            Self::Square { width, height } => Self::Hex {
                columns: width,
                rows: height,
            },
            Self::Hex { columns, rows } => Self::Square {
                width: columns,
                height: rows,
            },
        }
    }
}

impl fmt::Display for Grid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (columns, rows) = self.playable_size();
        match self {
            Self::Square { .. } => write!(f, "Square {columns}x{rows}"),
            Self::Hex { .. } => write!(f, "Hex {columns}x{rows}"),
        }
    }
}

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

    fn flip(self) -> Self {
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

    /// The neighbour of this cell in `direction` on `grid`.
    pub fn shifted(self, grid: Grid, direction: Direction) -> Self {
        let step = grid.step(direction);
        Self::new(self.x + step.x, self.y + step.y)
    }
}

/// A way off a cell, on a compass. Which of these a grid steps in is the
/// grid's own business: a square grid moves on the four cardinal ones, a
/// hex lattice on six, none of them straight up or down.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
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
    grid: Grid,
    cells: Vec<Cell>,
    tokens: Vec<Token>,
}

impl Board {
    /// A board on `grid` with every playable cell colored at random.
    pub fn random(grid: Grid, rng: &mut impl Rng) -> Self {
        assert!(
            grid.playable_count() >= 2,
            "the board needs at least two playable cells"
        );

        loop {
            let mut board = Self::empty(grid);

            for coord in grid.playable_coords() {
                let cell = match rng.random_range(0..4) {
                    0 | 1 => Cell::Empty,
                    2 => Cell::Red,
                    _ => Cell::Green,
                };
                board.set_cell(coord, cell);
            }

            if Side::ALL.iter().all(|&side| board.colored_count(side) > 0) {
                return board;
            }
        }
    }

    /// A board on `grid` with no colors and no tokens: cells are colored
    /// with [`Board::set_cell`]. Intended for tests and scenario setup.
    pub fn from_grid(grid: Grid) -> Self {
        Self::empty(grid)
    }

    fn empty(grid: Grid) -> Self {
        let (width, height) = grid.storage_size();

        Self {
            grid,
            cells: vec![Cell::Empty; width * height],
            tokens: Vec::new(),
        }
    }

    /// Builds a square board from row strings using `.` for empty, `R`
    /// for red and `G` for green. The outer ring is left empty. Intended
    /// for tests and scenario setup.
    pub fn from_rows(rows: &[&str]) -> Self {
        let height = rows.len();
        let width = rows.first().map_or(0, |row| row.len());
        let mut board = Self::from_grid(Grid::Square { width, height });

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

    pub const fn grid(&self) -> Grid {
        self.grid
    }

    pub fn is_in_storage(&self, coord: Coord) -> bool {
        self.grid.contains_storage(coord)
    }

    pub fn is_playable_cell(&self, coord: Coord) -> bool {
        self.grid.contains_playable(coord)
    }

    /// Colors a cell. Only playable cells take a color; the outer ring
    /// stays empty. Intended for tests and scenario setup.
    pub fn set_cell(&mut self, coord: Coord, cell: Cell) {
        assert!(
            self.is_playable_cell(coord),
            "only playable cells are colored, and {coord:?} is not one"
        );
        let index = self.index(coord);
        self.cells[index] = cell;
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
        let directions = self.grid.directions();
        self.storage_coords().filter(move |&coord| {
            self.can_host_token(coord)
                && directions.iter().any(|&direction| {
                    self.legal_jump_from(coord, direction, &no_history)
                        .is_some()
                })
        })
    }

    /// Every cell inside the outer ring, row by row.
    fn playable_coords(&self) -> impl Iterator<Item = Coord> {
        self.grid.playable_coords()
    }

    /// Every cell of the storage grid, row by row.
    pub fn storage_coords(&self) -> impl Iterator<Item = Coord> {
        self.grid.storage_coords()
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

        let mut cursor = origin.shifted(self.grid, direction);
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
            cursor = cursor.shifted(self.grid, direction);
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

        self.grid
            .directions()
            .iter()
            .filter_map(|&direction| self.legal_jump_from(origin, direction, crossed_this_chain))
            .collect()
    }

    fn index(&self, coord: Coord) -> usize {
        debug_assert!(self.is_in_storage(coord));
        let (width, _) = self.grid.storage_size();
        coord.y as usize * width + coord.x as usize
    }

    fn can_host_token(&self, coord: Coord) -> bool {
        self.is_in_storage(coord)
            && self.cell_at(coord) == Some(Cell::Empty)
            && !self.is_occupied(coord)
    }

    /// Adds a token when the cell can host one. Intended for tests and
    /// scenario setup; gameplay uses `Game::try_place_initial_token`.
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
            self.set_cell(coord, cell.flip());
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
    /// Deals a random game on `grid`.
    pub fn new_random(grid: Grid, rng: &mut impl Rng) -> Self {
        let board = Board::random(grid, rng);
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

    pub fn legal_initial_placements(&self) -> Vec<Coord> {
        if self.phase == Phase::PlaceInitialToken {
            self.board.legal_initial_placements()
        } else {
            Vec::new()
        }
    }

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

    pub fn selectable_activated_tokens(&self) -> Vec<TokenId> {
        match self.phase {
            Phase::ChooseActivatedToken => self.movable_pending_tokens(),
            // Before the first hop the selection is still negotiable:
            // the other movable activated tokens glow as switch targets.
            Phase::ChooseJump if self.crossed_this_chain.is_empty() => {
                self.movable_pending_tokens()
            }
            _ => Vec::new(),
        }
    }

    #[must_use = "the move is not made when it returns false"]
    pub fn try_place_initial_token(&mut self, coord: Coord) -> bool {
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

    #[must_use = "the move is not made when it returns false"]
    pub fn try_jump(&mut self, direction: Direction) -> bool {
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

    /// Jumps the current token to `landing` if that cell is a legal
    /// landing for it. This is the click-level operation shared by local
    /// play and the session: a click names the cell that was clicked and
    /// is resolved against the live board, never re-interpreted from a
    /// possibly stale view.
    #[must_use = "the move is not made when it returns false"]
    pub fn try_jump_to(&mut self, landing: Coord) -> bool {
        let Some(jump) = self
            .legal_current_jumps()
            .into_iter()
            .find(|jump| jump.landing == landing)
        else {
            return false;
        };
        self.try_jump(jump.direction)
    }

    /// Chooses which activated token to jump with. Beyond the
    /// `ChooseActivatedToken` phase this also accepts a switch during
    /// `ChooseJump` before the first hop: the selection only locks once
    /// jumping starts. A switch returns the previously selected token to
    /// the pending set, so the turn still asks for it afterwards.
    #[must_use = "the move is not made when it returns false"]
    pub fn try_select_activated_token(&mut self, token: TokenId) -> bool {
        let switching = self.phase == Phase::ChooseJump && self.crossed_this_chain.is_empty();
        if self.phase != Phase::ChooseActivatedToken && !switching {
            return false;
        }
        if !self.pending_activated.contains(&token)
            || !self.movable_pending_tokens().contains(&token)
        {
            return false;
        }

        if switching && let Some(previous) = self.current_token {
            self.pending_activated.push(previous);
        }
        self.pending_activated.retain(|pending| *pending != token);
        self.current_token = Some(token);
        self.crossed_this_chain.clear();
        self.phase = Phase::ChooseJump;
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
        let directions = self.board.grid.directions();
        for &coord in crossed {
            for &direction in directions {
                let neighbor = coord.shifted(self.board.grid, direction);
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

    /// A hex board with the given colored cells.
    fn hex_board(cells: &[(Coord, Cell)]) -> Board {
        let mut board = Board::from_grid(Grid::HEX);
        for &(coord, cell) in cells {
            board.set_cell(coord, cell);
        }
        board
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
    fn selected_token_can_be_switched_before_the_first_hop() {
        let mut board = board_from_rows(&[".......", ".RR....", ".RR.R..", ".......", "GG....."]);
        let first = TokenId(10);
        let second = TokenId(11);
        assert!(board.add_token(first, Coord::new(1, 2)));
        assert!(board.add_token(second, Coord::new(1, 3)));
        let mut game = Game::from_board(board, Side::Red);

        // Green (fewer cells) places above the red column and jumps
        // through it (two compulsory hops), activating both waiting
        // tokens.
        assert!(game.try_place_initial_token(Coord::new(2, 1)));
        assert!(game.try_jump(Direction::Down));
        assert!(game.try_jump(Direction::Down));

        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        let selectable = game.selectable_activated_tokens();
        assert_eq!(selectable.len(), 2);
        assert!(selectable.contains(&first));
        assert!(selectable.contains(&second));

        // Selecting one does not lock the choice in yet: the other one
        // still glows as a switch target.
        assert!(game.try_select_activated_token(first));
        assert_eq!(game.current_token(), Some(first));
        assert_eq!(game.phase(), Phase::ChooseJump);
        assert!(game.selectable_activated_tokens().contains(&second));

        assert!(game.try_select_activated_token(second));
        assert_eq!(game.current_token(), Some(second));
        assert!(game.pending_activated_tokens().contains(&first));
        // The running selection is not its own switch target.
        assert!(!game.try_select_activated_token(second));

        // Once the chain has started hopping, the selection locks.
        assert!(game.try_jump(Direction::Right));
        assert_eq!(game.phase(), Phase::ChooseJump);
        assert!(game.selectable_activated_tokens().is_empty());
        assert!(!game.try_select_activated_token(first));

        // The chain runs out of jumps after the second hop, and the
        // remaining activated token is asked for as usual.
        assert!(game.try_jump(Direction::Right));
        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert!(game.try_select_activated_token(first));
    }

    #[test]
    fn crossing_activates_cardinal_old_tokens_once() {
        let mut board = board_from_rows(&["...", ".R.", "R.."]);
        let cardinal = TokenId(10);
        let diagonal = TokenId(11);
        assert!(board.add_token(cardinal, Coord::new(2, 1)));
        assert!(board.add_token(diagonal, Coord::new(1, 1)));
        let mut game = Game::from_board(board, Side::Red);

        assert!(game.try_place_initial_token(Coord::new(1, 2)));
        assert!(game.try_jump(Direction::Right));

        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert_eq!(game.pending_activated_tokens(), &[cardinal]);
        assert_eq!(game.selectable_activated_tokens(), vec![cardinal]);
    }

    #[test]
    fn matching_color_wins_even_when_the_other_player_made_the_jump() {
        let board = board_from_rows(&["R.G"]);
        let mut game = Game::from_board(board, Side::Green);

        assert_eq!(game.current_player(), Side::Green);
        assert!(game.try_place_initial_token(Coord::new(4, 1)));
        assert!(game.try_jump(Direction::Left));

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

        assert!(game.try_place_initial_token(Coord::new(0, 1)));
        assert!(game.try_jump(Direction::Right));

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

        assert!(game.try_place_initial_token(Coord::new(1, 1)));
        assert!(game.try_jump(Direction::Right));

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
        assert!(game.try_place_initial_token(Coord::new(0, 1)));
        assert!(game.try_jump(Direction::Right));

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
        for coord in board.grid().storage_coords() {
            if !board.is_playable_cell(coord) {
                assert!(board.add_token(TokenId(id), coord));
                id += 1;
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

    #[test]
    fn a_hex_board_is_a_rhombus_with_a_full_ring() {
        let (width, height) = Grid::HEX.storage_size();
        assert_eq!((width, height), (6, 7));
        assert_eq!(Grid::HEX.playable_count(), Grid::SQUARE.playable_count());
        assert_eq!(Grid::HEX.other(), Grid::SQUARE);

        let board = Board::from_grid(Grid::HEX);
        for coord in Grid::HEX.playable_coords() {
            for direction in Grid::HEX.directions() {
                assert!(
                    board.is_in_storage(coord.shifted(Grid::HEX, *direction)),
                    "a playable cell's every neighbour is on the board"
                );
            }
        }

        // Every ring cell touches the playable area, so a token there is
        // always within reach of a chain.
        for coord in Grid::HEX
            .storage_coords()
            .filter(|c| !Grid::HEX.contains_playable(*c))
        {
            assert!(
                Grid::HEX
                    .directions()
                    .iter()
                    .any(|&direction| Grid::HEX
                        .contains_playable(coord.shifted(Grid::HEX, direction))),
                "the ring cell {coord:?} should touch a playable cell"
            );
        }
    }

    #[test]
    fn every_grid_steps_the_same_cell_back_again() {
        for grid in [Grid::SQUARE, Grid::HEX] {
            for direction in grid.directions() {
                let back = match direction {
                    Direction::Up | Direction::Down => continue,
                    Direction::Right => Direction::Left,
                    Direction::Left => Direction::Right,
                    Direction::UpRight => Direction::DownLeft,
                    Direction::UpLeft => Direction::DownRight,
                    Direction::DownRight => Direction::UpLeft,
                    Direction::DownLeft => Direction::UpRight,
                };
                let start = Coord::new(2, 2);
                let there = start.shifted(grid, *direction);
                assert_ne!(there, start, "{grid} should step somewhere");
                assert_eq!(there.shifted(grid, back), start);
            }
        }
    }

    #[test]
    fn hex_runs_run_on_the_diagonals() {
        // A hex board has no cell straight above another: its columns
        // are diagonals.
        let mut board = hex_board(&[
            (Coord::new(2, 3), Cell::Red),
            (Coord::new(3, 2), Cell::Green),
        ]);
        let token = TokenId(1);
        assert!(board.add_token(token, Coord::new(1, 4)));

        let jumps = board.legal_jumps_for_token(token, &BTreeSet::new());
        let jump = jumps
            .iter()
            .find(|jump| jump.direction == Direction::UpRight)
            .expect("the diagonal run should be jumpable");

        assert_eq!(jump.crossed, vec![Coord::new(2, 3), Coord::new(3, 2)]);
        assert_eq!(jump.landing, Coord::new(4, 1));
    }

    #[test]
    fn a_hex_run_can_land_in_the_outer_ring() {
        let mut board = hex_board(&[
            (Coord::new(2, 2), Cell::Red),
            (Coord::new(2, 3), Cell::Red),
            (Coord::new(2, 4), Cell::Red),
            (Coord::new(2, 5), Cell::Red),
        ]);
        let token = TokenId(1);
        assert!(board.add_token(token, Coord::new(2, 1)));

        let jumps = board.legal_jumps_for_token(token, &BTreeSet::new());
        let jump = jumps
            .iter()
            .find(|jump| jump.direction == Direction::DownRight)
            .expect("the column should be jumpable");

        assert_eq!(jump.landing, Coord::new(2, 6));
        assert!(!board.is_playable_cell(jump.landing));
    }

    #[test]
    fn crossing_activates_hex_neighbors_a_square_grid_would_miss() {
        let mut board = hex_board(&[
            (Coord::new(3, 1), Cell::Green),
            (Coord::new(1, 2), Cell::Red),
            (Coord::new(1, 3), Cell::Red),
            (Coord::new(4, 4), Cell::Red),
        ]);
        // Up and right of a crossed cell: neighbours on a hex board, a
        // diagonal step apart on a square one.
        let waiting = TokenId(10);
        assert!(board.add_token(waiting, Coord::new(2, 1)));
        let mut game = Game::from_board(board, Side::Green);

        assert!(game.try_place_initial_token(Coord::new(1, 1)));
        assert!(game.try_jump(Direction::DownRight));

        assert_eq!(game.phase(), Phase::ChooseActivatedToken);
        assert_eq!(game.pending_activated_tokens(), &[waiting]);
        assert_eq!(game.selectable_activated_tokens(), vec![waiting]);
    }

    #[test]
    fn hex_games_round_trip_through_json_with_their_grid() {
        let mut board = hex_board(&[
            (Coord::new(1, 1), Cell::Green),
            (Coord::new(2, 2), Cell::Red),
            (Coord::new(2, 3), Cell::Red),
            (Coord::new(4, 5), Cell::Green),
        ]);
        assert!(board.add_token(TokenId(5), Coord::new(3, 1)));
        let mut game = Game::from_board(board, Side::Green);
        assert!(game.try_place_initial_token(Coord::new(2, 1)));
        assert!(game.try_jump(Direction::DownRight));

        let json = serde_json::to_string(&game).unwrap();
        let back: Game = serde_json::from_str(&json).unwrap();
        assert_eq!(back, game);
        assert_eq!(back.board().grid(), Grid::HEX);
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}
