//! Transport-agnostic match session: seats at most two connections, owns
//! the game, and turns every client action into the messages that each
//! seat should receive. Shared by the dedicated WebSocket server and, in
//! the wasm build, by the hosting browser client itself.

use rand::rng;

use crate::{
    BOARD_HEIGHT, BOARD_WIDTH, ClientMessage, Coord, Game, Phase, ServerMessage, Side, Status,
};

/// Identifies one connection to the session. Only used for identity: a
/// disconnect from an id that no longer occupies the seat is ignored, so
/// a stale disconnect cannot evict a reconnected player.
pub type ConnId = u64;

#[derive(Clone, Debug, PartialEq)]
pub enum ClaimOutcome {
    Seated {
        side: Side,
        /// Messages for every seated side, starting with the newcomer's
        /// `Welcome`.
        messages: Vec<(Side, ServerMessage)>,
    },
    Full,
}

/// Seats are purely connection-based: `Some(conn)` while a client is
/// connected, `None` as soon as it disconnects. Nothing identifies a
/// player across connections — the next connection simply takes the next
/// free slot.
#[derive(Default)]
pub struct Session {
    seats: [Option<ConnId>; 2],
    game: Option<Game>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seats the connection in the next free slot, starts a game once both
    /// seats are taken, and reports the side plus the messages to deliver
    /// (the newcomer's `Welcome` followed by the current state).
    pub fn claim(&mut self, conn: ConnId) -> ClaimOutcome {
        let Some(side) = self.free_side() else {
            return ClaimOutcome::Full;
        };

        self.seats[seat_index(side)] = Some(conn);

        if self.seats.iter().all(Option::is_some) && self.game.is_none() {
            self.game = Some(new_game());
        }

        let mut messages = vec![(side, ServerMessage::Welcome { side })];
        messages.extend(self.state_messages());
        ClaimOutcome::Seated { side, messages }
    }

    /// Applies a client action, answering with an error for the acting
    /// side or broadcasting the new state to every seated side.
    pub fn handle_message(
        &mut self,
        side: Side,
        msg: ClientMessage,
    ) -> Vec<(Side, ServerMessage)> {
        match msg {
            ClientMessage::Place { x, y } => {
                self.apply_action(side, |game| game.try_place_initial_token(Coord::new(x, y)))
            }
            ClientMessage::Jump { direction } => {
                self.apply_action(side, |game| game.try_jump(direction))
            }
            ClientMessage::SelectToken { id } => {
                self.apply_action(side, |game| game.try_select_activated_token(id))
            }
            ClientMessage::NewGame => {
                let Some(game) = &self.game else {
                    return error(side, "no active game");
                };
                if game.phase() != Phase::GameOver {
                    return error(side, "the game is not over");
                }
                if !self.opponent_present(side) {
                    return error(side, "opponent is not connected");
                }
                self.game = Some(new_game());
                self.state_messages()
            }
            ClientMessage::Leave => {
                if self.game.is_none() {
                    return error(side, "there is no game to leave");
                }
                if self.opponent_present(side) {
                    return error(side, "opponent is still connected");
                }
                // The opponent's seat is already free (they disconnected),
                // so abandoning the game is all that is left to do.
                self.game = None;
                self.state_messages()
            }
        }
    }

    /// Frees the seat if `conn` still occupies it and broadcasts the
    /// resulting state.
    pub fn disconnect(&mut self, side: Side, conn: ConnId) -> Vec<(Side, ServerMessage)> {
        let index = seat_index(side);
        if self.seats[index] == Some(conn) {
            self.seats[index] = None;
        }
        self.state_messages()
    }

    /// The connection currently seated on `side`, if any.
    pub fn seat_conn(&self, side: Side) -> Option<ConnId> {
        self.seats[seat_index(side)]
    }

    /// The session's game, if one is running.
    pub fn game(&self) -> Option<&Game> {
        self.game.as_ref()
    }

    /// Replaces the session's game with `game` — e.g. restored from a
    /// state snapshot. The next turn continues from it unchanged.
    pub fn set_game(&mut self, game: Game) {
        self.game = Some(game);
    }

    fn apply_action(
        &mut self,
        side: Side,
        action: impl FnOnce(&mut Game) -> bool,
    ) -> Vec<(Side, ServerMessage)> {
        match &self.game {
            None => return error(side, "no active game"),
            Some(game) if game.phase() == Phase::GameOver => {
                return error(side, "the game is over");
            }
            Some(game) if game.current_player() != side => return error(side, "not your turn"),
            Some(_) => {}
        }
        if !self.opponent_present(side) {
            return error(side, "opponent is not connected");
        }

        let game = self.game.as_mut().expect("game existence checked above");
        if !action(game) {
            return error(side, "illegal action");
        }
        self.state_messages()
    }

    fn free_side(&self) -> Option<Side> {
        [Side::Red, Side::Green]
            .into_iter()
            .find(|side| self.seats[seat_index(*side)].is_none())
    }

    fn opponent_present(&self, side: Side) -> bool {
        self.seats[seat_index(side.other())].is_some()
    }

    fn status(&self) -> Status {
        if self.game.is_none() {
            return Status::Waiting;
        }
        if self.seats.iter().any(Option::is_none) {
            return Status::Paused;
        }
        if self
            .game
            .as_ref()
            .is_some_and(|game| game.phase() == Phase::GameOver)
        {
            return Status::Finished;
        }
        Status::Playing
    }

    fn state_messages(&self) -> Vec<(Side, ServerMessage)> {
        let status = self.status();
        [Side::Red, Side::Green]
            .into_iter()
            .filter_map(|side| {
                let message = ServerMessage::State {
                    game: self.game.clone(),
                    side,
                    opponent_present: self.opponent_present(side),
                    status,
                };
                self.seats[seat_index(side)]
                    .is_some()
                    .then_some((side, message))
            })
            .collect()
    }
}

fn seat_index(side: Side) -> usize {
    match side {
        Side::Red => 0,
        Side::Green => 1,
    }
}

fn new_game() -> Game {
    Game::new_random(BOARD_WIDTH, BOARD_HEIGHT, &mut rng())
}

fn error(side: Side, reason: &str) -> Vec<(Side, ServerMessage)> {
    vec![(
        side,
        ServerMessage::Error {
            reason: reason.to_owned(),
        },
    )]
}

#[cfg(test)]
mod tests {
    use crate::{Board, TokenId};

    use super::*;

    /// A live 1x3 board where red starts and has a legal first placement.
    fn live_game() -> Game {
        Game::from_board(Board::from_rows(&["R.G"]), Side::Red)
    }

    /// A dead 2x2 board whose outer ring is blocked by tokens, so the game
    /// has already ended with a red majority.
    fn finished_game() -> Game {
        let mut board = Board::from_rows(&["RR", "RG"]);
        let mut id = 100_u32;
        for y in 0..board.storage_height() as i32 {
            for x in 0..board.storage_width() as i32 {
                let coord = Coord::new(x, y);
                if !board.is_playable_cell(coord) {
                    assert!(board.add_token(TokenId(id), coord));
                    id += 1;
                }
            }
        }
        Game::from_board(board, Side::Red)
    }

    fn seated(outcome: ClaimOutcome) -> (Side, Vec<(Side, ServerMessage)>) {
        match outcome {
            ClaimOutcome::Seated { side, messages } => (side, messages),
            ClaimOutcome::Full => panic!("expected a free seat"),
        }
    }

    /// Keeps only the messages addressed to `side`.
    fn for_side(messages: &[(Side, ServerMessage)], side: Side) -> Vec<&ServerMessage> {
        messages
            .iter()
            .filter(|(message_side, _)| *message_side == side)
            .map(|(_, message)| message)
            .collect()
    }

    fn last_state(messages: &[&ServerMessage]) -> (Option<Game>, bool, Status) {
        messages
            .iter()
            .rev()
            .find_map(|message| match message {
                ServerMessage::State {
                    game,
                    opponent_present,
                    status,
                    ..
                } => Some((game.clone(), *opponent_present, *status)),
                _ => None,
            })
            .expect("expected a state message")
    }

    fn err(messages: &[&ServerMessage]) -> String {
        messages
            .iter()
            .find_map(|message| match message {
                ServerMessage::Error { reason } => Some(reason.clone()),
                _ => None,
            })
            .expect("expected an error message")
    }

    #[test]
    fn first_seat_is_red_second_is_green() {
        let mut session = Session::new();

        let (side, welcome) = seated(session.claim(1));
        assert_eq!(side, Side::Red);
        assert!(
            matches!(welcome.first(), Some((Side::Red, ServerMessage::Welcome { side: Side::Red }))),
            "the newcomer is welcomed first"
        );
        let (_, opponent_present, status) = last_state(&for_side(&welcome, Side::Red));
        assert!(!opponent_present);
        assert_eq!(status, Status::Waiting);

        let (side, messages) = seated(session.claim(2));
        assert_eq!(side, Side::Green);

        let (red_game, _, red_status) = last_state(&for_side(&messages, Side::Red));
        let (green_game, opponent_present, green_status) = last_state(&for_side(&messages, Side::Green));
        assert!(red_game.is_some());
        assert_eq!(red_game, green_game);
        assert!(opponent_present);
        assert_eq!(red_status, Status::Playing);
        assert_eq!(green_status, Status::Playing);
    }

    #[test]
    fn third_player_is_rejected_while_both_seats_are_taken() {
        let mut session = Session::new();
        assert!(matches!(session.claim(1), ClaimOutcome::Seated { .. }));
        assert!(matches!(session.claim(2), ClaimOutcome::Seated { .. }));

        assert_eq!(session.claim(3), ClaimOutcome::Full);
    }

    #[test]
    fn next_connection_takes_the_free_slot_and_resumes_the_game() {
        let mut session = Session::new();
        seated(session.claim(1));
        seated(session.claim(2));
        session.game = Some(live_game());
        let held = session.game.clone();

        let paused = session.disconnect(Side::Red, 1);
        assert!(
            for_side(&paused, Side::Red).is_empty(),
            "the seatless client gets nothing"
        );
        let (_, opponent_present, status) = last_state(&for_side(&paused, Side::Green));
        assert!(!opponent_present);
        assert_eq!(status, Status::Paused);

        // A brand-new connection takes the free slot; the game continues.
        let (side, messages) = seated(session.claim(3));
        assert_eq!(side, Side::Red);

        let (game, opponent_present, status) = last_state(&for_side(&messages, Side::Red));
        assert_eq!(game, held, "the paused game is resumed unchanged");
        assert!(opponent_present);
        assert_eq!(status, Status::Playing);
    }

    #[test]
    fn moves_are_rejected_out_of_turn_and_accepted_in_turn() {
        let mut session = Session::new();
        seated(session.claim(1));
        seated(session.claim(2));
        session.game = Some(live_game());

        // The live fixture makes red the starter.
        let reply = session.handle_message(Side::Green, ClientMessage::Place { x: 0, y: 1 });
        assert_eq!(err(&for_side(&reply, Side::Green)), "not your turn");

        // A jump before any token has been placed is an illegal action.
        let reply = session.handle_message(
            Side::Red,
            ClientMessage::Jump {
                direction: crate::Direction::Up,
            },
        );
        assert_eq!(err(&for_side(&reply, Side::Red)), "illegal action");

        // Red's legal placement is accepted and broadcast to both seats.
        let reply = session.handle_message(Side::Red, ClientMessage::Place { x: 2, y: 1 });
        assert!(
            !reply
                .iter()
                .any(|(_, message)| matches!(message, ServerMessage::Error { .. }))
        );
        assert_eq!(reply.len(), 2, "both seats receive the new state");
    }

    #[test]
    fn leave_abandons_a_paused_game_and_a_new_pairing_starts_fresh() {
        let mut session = Session::new();
        seated(session.claim(1));
        seated(session.claim(2));
        session.game = Some(live_game());

        let paused = session.disconnect(Side::Green, 2);
        let (_, _, status) = last_state(&for_side(&paused, Side::Red));
        assert_eq!(status, Status::Paused);

        // Leave is allowed while the opponent is absent.
        let messages = session.handle_message(Side::Red, ClientMessage::Leave);
        let (_, _, status) = last_state(&for_side(&messages, Side::Red));
        assert_eq!(status, Status::Waiting);
        assert!(session.game.is_none());

        // Leaving again with no game is refused.
        let reply = session.handle_message(Side::Red, ClientMessage::Leave);
        assert_eq!(err(&for_side(&reply, Side::Red)), "there is no game to leave");

        // The next client fills the free slot and a fresh game starts.
        let (side, _) = seated(session.claim(3));
        assert_eq!(side, Side::Green);
        assert!(session.game.is_some());

        // Leave is refused while the opponent is connected.
        let reply = session.handle_message(Side::Red, ClientMessage::Leave);
        assert_eq!(err(&for_side(&reply, Side::Red)), "opponent is still connected");
    }

    #[test]
    fn new_game_requires_a_finished_game_and_a_connected_opponent() {
        let mut session = Session::new();
        seated(session.claim(1));
        seated(session.claim(2));

        session.game = Some(live_game());
        let reply = session.handle_message(Side::Red, ClientMessage::NewGame);
        assert_eq!(err(&for_side(&reply, Side::Red)), "the game is not over");

        let finished = finished_game();
        let finished_json = serde_json::to_string(&finished).unwrap();
        session.game = Some(finished.clone());

        session.disconnect(Side::Green, 2);
        let reply = session.handle_message(Side::Red, ClientMessage::NewGame);
        assert_eq!(err(&for_side(&reply, Side::Red)), "opponent is not connected");

        // The next connection fills the free slot; the finished game is
        // held (and not replaced) until NewGame runs.
        seated(session.claim(3));

        let reply = session.handle_message(Side::Red, ClientMessage::NewGame);
        let (game, _, status) = last_state(&for_side(&reply, Side::Red));
        let game = game.expect("new game exists");
        assert_ne!(
            serde_json::to_string(&game).unwrap(),
            finished_json,
            "the finished game must be replaced"
        );
        assert_ne!(status, Status::Finished);
    }

    #[test]
    fn paused_game_survives_both_players_leaving() {
        let mut session = Session::new();
        seated(session.claim(1));
        seated(session.claim(2));
        session.game = Some(live_game());
        let held = session.game.clone();

        session.disconnect(Side::Red, 1);
        session.disconnect(Side::Green, 2);
        assert_eq!(session.game, held, "the game is held with nobody connected");
        assert_eq!(session.status(), Status::Paused);

        // Whoever fills the slots next inherits the paused game.
        let (first, _) = seated(session.claim(3));
        assert_eq!(first, Side::Red);
        let (second, messages) = seated(session.claim(4));
        assert_eq!(second, Side::Green);
        assert_eq!(session.game, held);

        let (game, _, status) = last_state(&for_side(&messages, Side::Green));
        assert_eq!(game, held);
        assert_eq!(status, Status::Playing);
    }

    #[test]
    fn stale_disconnects_do_not_evict_a_reconnected_player() {
        let mut session = Session::new();
        seated(session.claim(1));
        seated(session.claim(2));
        session.disconnect(Side::Red, 1);
        seated(session.claim(3));

        // The old connection's disconnect arrives late; the new occupant
        // of the red seat must keep it.
        let messages = session.disconnect(Side::Red, 1);
        assert_eq!(session.seat_conn(Side::Red), Some(3));
        let (_, opponent_present, status) = last_state(&for_side(&messages, Side::Green));
        assert!(opponent_present, "the reconnected player is still seated");
        assert_eq!(status, Status::Playing);
    }
}
