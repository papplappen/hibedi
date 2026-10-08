//! A match: the game, played exactly like a local hotseat game, plus
//! optionally the session it is shared through. The session never
//! touches the rules; it only decides whose clicks count (our seat) and
//! carries the resulting game states between the players.
//!
//! The [`Match`] resource is the single source of truth the views draw
//! from. It is only marked changed when something visible changed, so
//! views can redraw on `resource_changed::<Match>`.

use bevy::prelude::*;
use hibedi_core::{Coord, Game, Grid, Phase, Side};

use crate::session::Session;

pub fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, sync_session.run_if(resource_exists::<Match>))
        .add_systems(Update, new_game_on_key.run_if(resource_exists::<Match>));
}

/// Pulls in what the session received, marking the match changed only
/// if anything did.
fn sync_session(mut play: ResMut<Match>) {
    if play.bypass_change_detection().update() {
        play.set_changed();
    }
}

fn new_game_on_key(keys: Res<ButtonInput<KeyCode>>, mut play: ResMut<Match>) {
    if keys.just_pressed(KeyCode::KeyR) {
        play.start_new_game();
    }
    if keys.just_pressed(KeyCode::KeyH) {
        play.toggle_grid();
    }
}

#[derive(Resource)]
pub struct Match {
    /// `None` only online, until the first seat's game arrives.
    game: Option<Game>,
    session: Option<Box<dyn Session>>,
    /// Whether the other player is connected, as of the last update;
    /// `None` in hotseat.
    opponent_online: Option<bool>,
    /// The grid the next game is dealt on.
    grid: Grid,
}

impl Match {
    /// A hotseat game on this device.
    pub fn local() -> Self {
        Self {
            game: Some(new_game(Grid::SQUARE)),
            session: None,
            opponent_online: None,
            grid: Grid::SQUARE,
        }
    }

    /// Our seat in a shared game. A game already in progress arrives
    /// through the session; a new one is dealt by the first seat.
    pub fn online(session: Box<dyn Session>) -> Self {
        let mut play = Self {
            opponent_online: Some(session.everyone_online()),
            game: None,
            session: Some(session),
            grid: Grid::SQUARE,
        };
        play.adopt_received();
        if play.game.is_none() && play.my_side() == Some(Side::ALL[0]) {
            play.set_game(new_game(play.grid));
        }
        play
    }

    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    /// The grid the game is played on; `None` without a game.
    pub fn grid(&self) -> Option<Grid> {
        self.game.as_ref().map(|game| game.board().grid())
    }

    /// The side we play online; `None` in hotseat, where every side is
    /// played from this screen.
    pub fn my_side(&self) -> Option<Side> {
        Side::ALL.get(self.session.as_ref()?.seat()).copied()
    }

    /// Whether the other player is connected; `None` in hotseat.
    pub fn opponent_online(&self) -> Option<bool> {
        self.opponent_online
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

    /// Whether hints for the player to move are shown: always in
    /// hotseat, online only on our own turn.
    pub fn shows_hints(&self) -> bool {
        self.game.as_ref().is_some_and(|game| {
            self.my_side()
                .is_none_or(|side| side == game.current_player())
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

    /// Pulls in what the session received and takes steps that need no
    /// decision. Returns whether anything changed. Call once per frame.
    pub fn update(&mut self) -> bool {
        let online = self
            .session
            .as_ref()
            .map(|session| session.everyone_online());
        let presence_changed = online != self.opponent_online;
        self.opponent_online = online;
        let adopted = self.adopt_received();
        let selected = self.auto_select();
        presence_changed || adopted || selected
    }

    pub fn start_new_game(&mut self) {
        if self.can_start_new_game() {
            self.set_game(new_game(self.grid));
        }
    }

    /// Deals the next game on the other grid: the same rules on a hex
    /// lattice, or back on the square one.
    pub fn toggle_grid(&mut self) {
        self.grid = self.grid.other();
        self.start_new_game();
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

    /// Adopts the state the session received, if any.
    fn adopt_received(&mut self) -> bool {
        let Some(state) = self.session.as_mut().and_then(|session| session.receive()) else {
            return false;
        };
        match serde_json::from_value(state) {
            Ok(game) => {
                self.game = Some(game);
                true
            }
            Err(error) => {
                warn!("ignoring an unreadable game state: {error}");
                false
            }
        }
    }

    /// With exactly one token to choose there is no decision to make:
    /// select it right away.
    fn auto_select(&mut self) -> bool {
        if !self.can_move() {
            return false;
        }
        let Some(game) = &mut self.game else {
            return false;
        };
        if game.phase() == Phase::ChooseActivatedToken
            && let [only] = game.selectable_activated_tokens()[..]
            && game.try_select_activated_token(only)
        {
            self.publish();
            return true;
        }
        false
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

fn new_game(grid: Grid) -> Game {
    Game::new_random(grid, &mut rand::rng())
}
