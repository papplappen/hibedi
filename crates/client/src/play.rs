//! A match: the game, played exactly like a local hotseat game, plus
//! optionally the session it is shared through. The session never
//! touches the rules; it only decides whose clicks count (our seat) and
//! carries the resulting game states between the players.

use hibedi_core::{BOARD_HEIGHT, BOARD_WIDTH, Coord, Game, Phase, Side};

use crate::session::Session;

pub struct Match {
    /// `None` only online, until the first seat's game arrives.
    game: Option<Game>,
    session: Option<Box<dyn Session>>,
}

impl Match {
    /// A hotseat game on this device.
    pub fn local() -> Self {
        Self {
            game: Some(new_game()),
            session: None,
        }
    }

    /// Our seat in a shared game. A game already in progress arrives
    /// through the session; a new one is dealt by the first seat.
    pub fn online(session: Box<dyn Session>) -> Self {
        let mut play = Self {
            game: None,
            session: Some(session),
        };
        play.adopt_received();
        if play.game.is_none() && play.my_side() == Some(Side::ALL[0]) {
            play.set_game(new_game());
        }
        play
    }

    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    /// The side we play online; `None` in hotseat, where every side is
    /// played from this screen.
    pub fn my_side(&self) -> Option<Side> {
        Side::ALL.get(self.session.as_ref()?.seat()).copied()
    }

    /// Whether the other player is connected; `None` in hotseat.
    pub fn opponent_online(&self) -> Option<bool> {
        self.session
            .as_ref()
            .map(|session| session.everyone_online())
    }

    /// Whether the board takes our clicks now.
    pub fn can_move(&self) -> bool {
        self.game.as_ref().is_some_and(|game| {
            game.result().is_none()
                && self
                    .opponent_online()
                    .is_none_or(|online| online && self.my_side() == Some(game.current_player()))
        })
    }

    /// Hotseat restarts any time; online, either player may deal the next
    /// game once the current one is over.
    pub fn can_start_new_game(&self) -> bool {
        match self.opponent_online() {
            None => true,
            Some(online) => {
                online
                    && self
                        .game
                        .as_ref()
                        .is_some_and(|game| game.result().is_some())
            }
        }
    }

    /// Adopts what the session received and takes steps that need no
    /// decision. Call once per frame.
    pub fn update(&mut self) {
        self.adopt_received();
        self.auto_select();
    }

    pub fn start_new_game(&mut self) {
        if self.can_start_new_game() {
            self.set_game(new_game());
        }
    }

    /// Applies a click on a board cell.
    pub fn click(&mut self, coord: Coord) {
        if !self.can_move() {
            return;
        }
        let Some(game) = &mut self.game else {
            return;
        };
        let token = game.board().token_at(coord);
        let changed = match game.phase() {
            Phase::PlaceInitialToken => game.try_place_initial_token(coord),
            // A landing cell hops; another glowing token takes over the
            // turn instead (allowed until the first hop).
            Phase::ChooseJump => {
                game.try_jump_to(coord)
                    || token.is_some_and(|token| game.try_select_activated_token(token))
            }
            Phase::ChooseActivatedToken => {
                token.is_some_and(|token| game.try_select_activated_token(token))
            }
            Phase::GameOver => false,
        };
        if changed {
            self.publish();
        }
    }

    fn adopt_received(&mut self) {
        let Some(state) = self.session.as_mut().and_then(|session| session.receive()) else {
            return;
        };
        match serde_json::from_value(state) {
            Ok(game) => self.game = Some(game),
            Err(error) => eprintln!("ignoring an unreadable game state: {error}"),
        }
    }

    /// With exactly one token to choose there is no decision to make:
    /// select it right away.
    fn auto_select(&mut self) {
        if !self.can_move() {
            return;
        }
        let Some(game) = &mut self.game else {
            return;
        };
        if game.phase() == Phase::ChooseActivatedToken
            && let [only] = game.selectable_activated_tokens()[..]
            && game.try_select_activated_token(only)
        {
            self.publish();
        }
    }

    fn set_game(&mut self, game: Game) {
        self.game = Some(game);
        self.publish();
    }

    fn publish(&mut self) {
        if let (Some(session), Some(game)) = (&mut self.session, &self.game) {
            session.publish(serde_json::to_value(game).expect("games always serialize"));
        }
    }
}

fn new_game() -> Game {
    // `macroquad::prelude` re-exports quad_rand as `rand`; name the crate.
    Game::new_random(BOARD_WIDTH, BOARD_HEIGHT, &mut ::rand::rng())
}
