//! Computer players. A bot plays like anyone else: shown a game in which
//! it is to move, it picks one of the [`Game::actions`], one decision at
//! a time. A bot that plans further ahead (a whole turn, or several) can
//! keep its plan between calls.
//!
//! Every bot is listed in [`BotKind`], which names it to players and
//! builds it.

use hibedi_core::{Action, Game, GameResult, Side};

/// A computer player.
pub trait Bot: Send + Sync {
    /// The action to take in `game`, where the bot is to move; `None`
    /// only when there is none (the game is over).
    fn choose(&mut self, game: &Game) -> Option<Action>;
}

/// Every bot there is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "snake_case")]
pub enum BotKind {
    /// Takes the hop that wins it the most cells right away.
    Greedy,
}

impl BotKind {
    pub const ALL: [Self; 1] = [Self::Greedy];

    /// What players call it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Greedy => "Greedy",
        }
    }

    /// How it plays, in a sentence.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Greedy => "Takes the hop that wins it the most cells right away.",
        }
    }

    pub fn build(self) -> Box<dyn Bot> {
        match self {
            Self::Greedy => Box::new(Greedy),
        }
    }
}

/// Looks one hop ahead: takes the action leaving it the most cells of its
/// color, or a win. Placing or choosing a token counts as good as the
/// best first hop it allows. Equally good actions go to the first one
/// listed.
pub struct Greedy;

impl Bot for Greedy {
    fn choose(&mut self, game: &Game) -> Option<Action> {
        let side = game.current_player();
        let mut best: Option<(Action, i64)> = None;
        for action in game.actions() {
            // Taking back changes nothing on the board.
            if action == Action::TakeBack {
                continue;
            }
            let score = best_after(game, action, side, true);
            if best.is_none_or(|(_, best)| score > best) {
                best = Some((action, score));
            }
        }
        best.map(|(action, _)| action)
    }
}

/// The outcome for `side` of taking `action`; with `hop`, an action that
/// does not hop (placing or choosing a token) is rated by the best hop
/// it allows.
fn best_after(game: &Game, action: Action, side: Side, hop: bool) -> i64 {
    let mut next = game.clone();
    assert!(next.apply(action), "bots only take listed actions");
    let hops = matches!(action, Action::Jump(_));
    if hops || !hop || next.result().is_some() {
        return outcome(&next, side);
    }
    next.actions()
        .into_iter()
        .filter(|action| matches!(action, Action::Jump(_)))
        .map(|action| best_after(&next, action, side, false))
        .max()
        .unwrap_or_else(|| outcome(&next, side))
}

/// How good a position is for `side`: its cells, or a decided game.
fn outcome(game: &Game, side: Side) -> i64 {
    match game.result() {
        Some(GameResult::Winner { side: winner, .. }) if winner == side => i64::MAX,
        Some(GameResult::Winner { .. }) => i64::MIN,
        _ => game.board().colored_count(side) as i64,
    }
}

#[cfg(test)]
mod tests {
    use hibedi_core::{Board, Coord, Direction, Phase};

    use super::*;

    #[test]
    fn greedy_takes_a_winning_jump() {
        // Green to move (fewer cells). Hopping the red pair turns every
        // cell green; the first placement allowing that is between the
        // green cell and the red pair.
        let game = Game::from_board(Board::from_rows(&["G.RR."]), Side::Red);
        assert_eq!(game.current_player(), Side::Green);

        let mut game = game;
        let mut bot = Greedy;
        let place = bot.choose(&game).unwrap();
        assert_eq!(place, Action::Start(Coord::new(2, 1)));
        assert!(game.apply(place));
        assert_eq!(bot.choose(&game), Some(Action::Jump(Direction::Right)));
    }

    #[test]
    fn every_bot_finishes_games_against_itself() {
        let mut rng = rand::rng();
        for kind in BotKind::ALL {
            let mut bot = kind.build();
            for _ in 0..20 {
                let mut game = Game::new_random(4, 5, &mut rng);
                while game.phase() != Phase::GameOver {
                    let action = bot.choose(&game).expect("a running game has actions");
                    assert!(game.actions().contains(&action));
                    assert!(game.apply(action));
                }
                assert_eq!(bot.choose(&game), None);
            }
        }
    }
}
