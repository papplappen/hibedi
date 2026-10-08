//! A match: the game, and who plays each side. A side is played here
//! (by clicks and keys on this device), by a bot, or remotely by another
//! player, whose moves arrive through the session the game is shared
//! over. The session never touches the rules; it only carries game
//! states between the players.
//!
//! Every player decides the same way: by taking one of the game's
//! actions when their side is to move.
//!
//! The [`Match`] resource is the single source of truth the views draw
//! from. It is only marked changed when something visible changed, so
//! views can redraw on `resource_changed::<Match>`.

use bevy::prelude::*;
use hibedi_bots::Bot;
use hibedi_core::{Action, Coord, Game, Phase, Side};
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::session::Session;
use crate::settings::{Opponent, Settings, Setup};

pub fn plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        (sync_session, play_bots)
            .chain()
            .run_if(resource_exists::<Match>),
    );
}

/// Seconds a bot waits before each action, so its moves can be followed.
const BOT_PAUSE: f32 = 0.6;

/// Pulls in what the session received, marking the match changed only
/// if anything did.
fn sync_session(mut play: ResMut<Match>) {
    if play.bypass_change_detection().update() {
        play.set_changed();
    }
}

/// Lets a bot to move take its next action, after a pause.
fn play_bots(time: Res<Time>, mut play: ResMut<Match>, mut waited: Local<f32>) {
    if !play.bot_to_move() {
        *waited = 0.0;
        return;
    }
    *waited += time.delta_secs();
    if *waited >= BOT_PAUSE {
        *waited = 0.0;
        play.play_bot();
    }
}

/// Who makes the decisions for a side.
pub enum Player {
    /// Someone at this device.
    Here,
    Bot(Box<dyn Bot>),
    /// The other seat of the session.
    Remote,
}

#[derive(Resource)]
pub struct Match {
    /// `None` only online, until the first seat's game arrives.
    game: Option<Game>,
    /// Per side, in [`Side::ALL`] order.
    players: [Player; 2],
    /// What the game was dealt with (online, only its board counts).
    setup: Setup,
    session: Option<Box<dyn Session>>,
    /// Whether the other player is connected, as of the last update;
    /// `None` without a session.
    opponent_online: Option<bool>,
    /// The dealer's randomness.
    rng: StdRng,
}

impl Match {
    /// A game on this device, dealt as the settings say.
    pub fn local(settings: &Settings) -> Self {
        let mut play = Self {
            game: None,
            players: [Player::Here, Player::Here],
            setup: settings.setup(),
            session: None,
            opponent_online: None,
            rng: StdRng::from_rng(&mut rand::rng()),
        };
        play.deal(settings.setup());
        play
    }

    /// (Command line, native.) Deals from `seed`, if given, starting over with a new deal: the
    /// same seed deals the same boards, game after game.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_seed(mut self, seed: Option<u64>) -> Self {
        if let Some(seed) = seed {
            self.rng = StdRng::seed_from_u64(seed);
            self.deal(self.setup);
        }
        self
    }

    /// (Command line, native.) Continues `game`, if given, instead of a
    /// new deal.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_game(mut self, game: Option<Game>) -> Self {
        if let Some(game) = game {
            self.set_game(game);
        }
        self
    }

    /// Our seat in a shared game. A game already in progress arrives
    /// through the session; a new one is dealt by the first seat.
    pub fn online(session: Box<dyn Session>, settings: &Settings) -> Self {
        let mine = Side::ALL[session.seat()];
        let mut play = Self {
            game: None,
            players: Side::ALL.map(|side| {
                if side == mine {
                    Player::Here
                } else {
                    Player::Remote
                }
            }),
            setup: settings.setup(),
            opponent_online: Some(session.everyone_online()),
            session: Some(session),
            rng: StdRng::from_rng(&mut rand::rng()),
        };
        play.adopt_received();
        if play.game.is_none() && mine == Side::ALL[0] {
            play.deal(settings.setup());
        }
        play
    }

    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    pub fn player(&self, side: Side) -> &Player {
        &self.players[index(side)]
    }

    /// The side played on this device when it plays only one; `None`
    /// when both sides take turns at this screen.
    pub fn my_side(&self) -> Option<Side> {
        let mut here = Side::ALL
            .into_iter()
            .filter(|&side| matches!(self.player(side), Player::Here));
        match (here.next(), here.next()) {
            (Some(side), None) => Some(side),
            _ => None,
        }
    }

    /// Whether the other player is connected; `None` without a session.
    pub fn opponent_online(&self) -> Option<bool> {
        self.opponent_online
    }

    /// The side to move, while the game runs.
    fn to_move(&self) -> Option<Side> {
        self.game
            .as_ref()
            .filter(|game| game.result().is_none())
            .map(Game::current_player)
    }

    /// Whether the board takes our clicks and keys now.
    pub fn can_move(&self) -> bool {
        self.to_move()
            .is_some_and(|side| matches!(self.player(side), Player::Here))
            && self.opponent_online != Some(false)
    }

    /// Whether hints for the player to move are shown: whenever the side
    /// to move is played here.
    pub fn shows_hints(&self) -> bool {
        self.game
            .as_ref()
            .is_some_and(|game| matches!(self.player(game.current_player()), Player::Here))
    }

    /// On this device a new game may start any time; online, either
    /// player may deal the next game once the current one is over.
    pub fn can_start_new_game(&self) -> bool {
        match self.opponent_online {
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

    /// What the game was dealt with.
    pub fn setup(&self) -> Setup {
        self.setup
    }

    /// Deals a new game with `setup`, if one may start.
    pub fn start_new_game(&mut self, setup: Setup) {
        if self.can_start_new_game() {
            self.deal(setup);
        }
    }

    /// Deals a game with `setup`: its board, and, on this device, who
    /// plays which side. Online, the seats stay as the session has them.
    fn deal(&mut self, setup: Setup) {
        self.setup = setup;
        if self.session.is_none() {
            self.players = Side::ALL.map(|side| match setup.opponent {
                Opponent::Bot(bot) if side != setup.side => Player::Bot(bot.build()),
                _ => Player::Here,
            });
        }
        let game = Game::new_random(setup.board.width, setup.board.height, &mut self.rng);
        self.set_game(game);
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
        let started = self.start_the_only_token();
        presence_changed || adopted || started
    }

    /// The actions we may take now: the game's, while it takes our
    /// input.
    pub fn actions(&self) -> Vec<Action> {
        match &self.game {
            Some(game) if self.can_move() => game.actions(),
            _ => Vec::new(),
        }
    }

    /// Takes one of [`Match::actions`].
    pub fn act(&mut self, action: Action) {
        if self.can_move() {
            self.apply(action);
        }
    }

    /// Applies a click on a board cell: the action aimed at it, if any.
    /// Otherwise, before the token chosen hops, a click on the cell of
    /// another choice changes the choice to it, and a click on the token
    /// itself takes it back.
    pub fn click(&mut self, coord: Coord) {
        if !self.can_move() {
            return;
        }
        let Some(game) = &self.game else {
            return;
        };
        let aimed = game
            .actions()
            .into_iter()
            .find(|&action| game.action_cell(action) == Some(coord));
        if let Some(action) = aimed {
            self.apply(action);
        } else if game.start_options().contains(&coord) {
            let mut changed = game.clone();
            if changed.apply(Action::TakeBack) && changed.apply(Action::Start(coord)) {
                self.set_game(changed);
            }
        } else if game
            .current_token()
            .and_then(|token| game.board().token_position(token))
            == Some(coord)
        {
            self.apply(Action::TakeBack);
        }
    }

    /// The game's record in notation (see [`hibedi_core::notation`]).
    pub fn notation(&self) -> Option<String> {
        self.game.as_ref().map(hibedi_core::notation::write)
    }

    /// Whether a bot is to move.
    pub fn bot_to_move(&self) -> bool {
        self.to_move()
            .is_some_and(|side| matches!(self.player(side), Player::Bot(_)))
    }

    /// Has the bot to move take its next action.
    pub fn play_bot(&mut self) {
        let Some(side) = self.to_move() else {
            return;
        };
        let (Player::Bot(bot), Some(game)) = (&mut self.players[index(side)], &self.game) else {
            return;
        };
        if let Some(action) = bot.choose(game) {
            self.apply(action);
        }
    }

    fn apply(&mut self, action: Action) {
        if let Some(game) = &mut self.game
            && game.apply(action)
        {
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

    /// With a single token to start the next chain with, there is no
    /// decision to make: start it right away.
    fn start_the_only_token(&mut self) -> bool {
        if !self.can_move() {
            return false;
        }
        let Some(game) = &self.game else {
            return false;
        };
        if game.phase() == Phase::ChooseToken
            && let [only] = game.actions()[..]
        {
            self.apply(only);
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

const fn index(side: Side) -> usize {
    match side {
        Side::Red => 0,
        Side::Green => 1,
    }
}
