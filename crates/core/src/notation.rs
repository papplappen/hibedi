//! A compact text notation for whole games, in the spirit of chess's
//! PGN: a few tags describe the start, then every turn follows on a
//! line of its own.
//!
//! ```text
//! [Game "Hibe di Hopedi"]
//! [Board "R.GG/..R./G..R/.RG./RR.."]
//! [First "Green"]
//! [Result "Red"]
//!
//! 1. c1DD
//! 2. a4R e3LL
//! ```
//!
//! - `Board`: the playable cells row by row, top to bottom, rows
//!   separated by `/`: `.` empty, `R` red, `G` green.
//! - `Tokens` (only when the game starts with tokens on the board):
//!   their squares, separated by spaces.
//! - `First`: the side making the first turn.
//! - `Result`: `Red` or `Green` for a win, `Draw`, or `*` while the
//!   game runs.
//!
//! Squares count the board's outer ring in: columns are letters from
//! `a` on the left, rows numbers from `1` at the top, so `a1` is the
//! top-left corner of the outer ring and `b2` the top-left playable
//! cell.
//!
//! A turn is numbered and lists its chains: each is the square the
//! jumping token starts from, then its hops as `U`p, `R`ight, `D`own or
//! `L`eft. The first chain of a turn places a new token on its square;
//! later ones jump with the activated token on theirs. Choices taken
//! back are not recorded.

use std::fmt::{self, Write as _};

use crate::{Action, Board, Cell, Coord, Direction, Game, GameResult, Phase, Side, TokenId};

const GAME: &str = "Hibe di Hopedi";

/// The notation of a game, from its start to where it stands.
pub fn write(game: &Game) -> String {
    let start = game.start();
    let board = start.board();
    let mut text = String::new();
    let mut tag = |name: &str, value: &str| {
        writeln!(text, "[{name} \"{value}\"]").expect("strings take writes");
    };

    tag("Game", GAME);
    let rows: Vec<String> = (1..=board.height as i32)
        .map(|y| {
            (1..=board.width as i32)
                .map(|x| match board.cell_at(Coord::new(x, y)) {
                    Some(Cell::Red) => 'R',
                    Some(Cell::Green) => 'G',
                    _ => '.',
                })
                .collect()
        })
        .collect();
    tag("Board", &rows.join("/"));
    if !board.tokens().is_empty() {
        let mut tokens = board.tokens().to_vec();
        tokens.sort_by_key(|token| token.id);
        let squares: Vec<String> = tokens.iter().map(|token| square(token.position)).collect();
        tag("Tokens", &squares.join(" "));
    }
    tag("First", &game.first_player.to_string());
    tag(
        "Result",
        &match game.result() {
            Some(GameResult::Winner { side, .. }) => side.to_string(),
            Some(GameResult::Draw) => "Draw".to_owned(),
            None => "*".to_owned(),
        },
    );

    let mut replay = start;
    let mut turn = 0;
    for &action in game.history() {
        match action {
            // A blank line after the tags, then a line per turn.
            Action::Start(coord) if replay.places_token() => {
                turn += 1;
                write!(text, "\n{turn}. {}", square(coord))
            }
            Action::Start(coord) => write!(text, " {}", square(coord)),
            Action::Jump(direction) => text.write_char(direction_letter(direction)),
            Action::TakeBack => unreachable!("the history leaves out actions taken back"),
        }
        .expect("strings take writes");
        assert!(replay.apply(action), "the history replays to the game");
    }
    text.push('\n');
    text
}

/// Reads a game written in the notation, replaying it to where it
/// stands. Its [`Game::history`] holds every action, so the game can be
/// stepped through from [`Game::start`].
pub fn read(text: &str) -> Result<Game, Error> {
    let mut tags = Vec::new();
    let mut moves = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(tag) = line.strip_prefix('[') {
            let tag = tag.strip_suffix(']').ok_or(Error::Tag(line.to_owned()))?;
            let (name, value) = tag
                .split_once(' ')
                .ok_or_else(|| Error::Tag(line.to_owned()))?;
            let value = value
                .trim()
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .ok_or_else(|| Error::Tag(line.to_owned()))?;
            tags.push((name.to_owned(), value.to_owned()));
        } else {
            moves.extend(line.split_whitespace().map(str::to_owned));
        }
    }
    let tag = |name: &str| {
        tags.iter()
            .find(|(tag, _)| tag == name)
            .map(|(_, value)| value.as_str())
            .ok_or(Error::MissingTag(name.to_owned()))
    };

    let rows: Vec<&str> = tag("Board")?.split('/').collect();
    if rows.iter().any(|row| row.len() != rows[0].len())
        || rows[0].is_empty()
        || tag("Board")?
            .chars()
            .any(|c| !matches!(c, '.' | 'R' | 'G' | '/'))
    {
        return Err(Error::Board);
    }
    let mut board = Board::from_rows(&rows);
    if let Ok(tokens) = tag("Tokens") {
        for (id, at) in tokens.split_whitespace().enumerate() {
            let at = parse_square(at).ok_or_else(|| Error::Square(at.to_owned()))?;
            if !board.add_token(TokenId(id as u32 + 1), at) {
                return Err(Error::Square(square(at)));
            }
        }
    }
    let first = match tag("First")? {
        "Red" => Side::Red,
        "Green" => Side::Green,
        other => return Err(Error::Side(other.to_owned())),
    };

    let mut game = Game::starting(board, first);
    for word in moves {
        if let Some(number) = word.strip_suffix('.') {
            if number.parse::<u32>().is_err()
                || game.phase() != Phase::ChooseToken
                || !game.places_token()
            {
                return Err(Error::Move(word));
            }
            continue;
        }
        let split = word
            .find(|c: char| c.is_ascii_uppercase())
            .unwrap_or(word.len());
        let (at, hops) = word.split_at(split);
        let at = parse_square(at).ok_or_else(|| Error::Square(at.to_owned()))?;
        let mut actions = vec![Action::Start(at)];
        for letter in hops.chars() {
            let direction = Direction::ALL
                .into_iter()
                .find(|&direction| direction_letter(direction) == letter)
                .ok_or_else(|| Error::Move(word.clone()))?;
            actions.push(Action::Jump(direction));
        }
        for action in actions {
            if !game.apply(action) {
                return Err(Error::Move(word));
            }
        }
    }
    Ok(game)
}

/// What keeps a text from reading as a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Tag(String),
    MissingTag(String),
    Board,
    Side(String),
    Square(String),
    /// A chain that is not a legal move where it stands.
    Move(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag(line) => write!(f, "malformed tag: {line}"),
            Self::MissingTag(name) => write!(f, "missing tag: {name}"),
            Self::Board => f.write_str("malformed board"),
            Self::Side(side) => write!(f, "unknown side: {side}"),
            Self::Square(square) => write!(f, "bad square: {square}"),
            Self::Move(word) => write!(f, "illegal move: {word}"),
        }
    }
}

impl std::error::Error for Error {}

/// A square's name: column letter, then row number.
pub fn square(coord: Coord) -> String {
    let column = char::from(b'a' + coord.x as u8);
    format!("{column}{}", coord.y + 1)
}

fn parse_square(text: &str) -> Option<Coord> {
    let mut chars = text.chars();
    let column = chars.next().filter(char::is_ascii_lowercase)?;
    let row: i32 = chars.as_str().parse().ok()?;
    (row >= 1).then(|| Coord::new(i32::from(column as u8 - b'a'), row - 1))
}

const fn direction_letter(direction: Direction) -> char {
    match direction {
        Direction::Up => 'U',
        Direction::Right => 'R',
        Direction::Down => 'D',
        Direction::Left => 'L',
    }
}

#[cfg(test)]
mod tests {
    use rand::RngExt;

    use super::*;
    use crate::{BOARD_HEIGHT, BOARD_WIDTH};

    #[test]
    fn a_game_reads_back_as_written() {
        let mut board = Board::from_rows(&["RR.R", "G..."]);
        assert!(board.add_token(TokenId(7), Coord::new(5, 2)));
        let mut game = Game::from_board(board, Side::Red);
        assert!(game.apply(Action::Start(Coord::new(0, 1))));
        assert!(game.apply(Action::Jump(Direction::Right)));

        let text = write(&game);
        assert_eq!(
            text,
            "[Game \"Hibe di Hopedi\"]\n\
             [Board \"RR.R/G...\"]\n\
             [Tokens \"f3\"]\n\
             [First \"Green\"]\n\
             [Result \"*\"]\n\
             \n\
             1. a2R\n"
        );
        // Token ids are not written: tokens are known by their squares.
        let back = read(&text).unwrap();
        let positions =
            |game: &Game| Vec::from_iter(game.board().tokens().iter().map(|token| token.position));
        assert_eq!(positions(&back), positions(&game));
        assert_eq!(write(&back), text);
    }

    #[test]
    fn random_games_round_trip() {
        let mut rng = rand::rng();
        for _ in 0..50 {
            let mut game = Game::new_random(BOARD_WIDTH, BOARD_HEIGHT, &mut rng);
            while game.result().is_none() {
                let actions = game.actions();
                let action = actions[rng.random_range(0..actions.len())];
                assert!(game.apply(action));
            }
            let text = write(&game);
            let back = read(&text).unwrap_or_else(|error| panic!("{error} in\n{text}"));
            assert_eq!(back, game);
        }
    }

    #[test]
    fn illegal_moves_are_refused() {
        let text = "[Board \"RR.R/G...\"]\n[First \"Red\"]\n\n1. b2R\n";
        assert_eq!(read(text), Err(Error::Move("b2R".to_owned())));
    }
}
