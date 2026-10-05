use hibedi_core::{
    BOARD_HEIGHT, BOARD_WIDTH, ClientMessage, Coord, Game, Phase, ServerMessage, Side, Status,
};
use rand::RngExt;
use tokio::sync::{mpsc, oneshot};

pub type Tx = mpsc::UnboundedSender<ServerMessage>;

#[derive(Debug)]
pub enum Command {
    Claim {
        token: Option<String>,
        tx: Tx,
        reply: oneshot::Sender<ClaimOutcome>,
    },
    Sync,
    Message {
        side: Side,
        msg: ClientMessage,
    },
    Disconnect {
        side: Side,
        tx: Tx,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClaimOutcome {
    Seated { side: Side, token: String },
    Full,
}

#[derive(Default)]
struct Seat {
    token: Option<String>,
    tx: Option<Tx>,
}

#[derive(Default)]
pub struct Session {
    seats: [Seat; 2],
    game: Option<Game>,
}

fn seat_index(side: Side) -> usize {
    match side {
        Side::Red => 0,
        Side::Green => 1,
    }
}

fn generate_token() -> String {
    format!("{:032x}", rand::rng().random::<u128>())
}

fn new_game() -> Game {
    Game::new_random(BOARD_WIDTH, BOARD_HEIGHT, &mut rand::rng())
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles one command, delivering every resulting message to the
    /// receiving seat's channel, and returns the same messages so callers
    /// (and tests) can inspect them.
    pub fn handle(&mut self, command: Command) -> Vec<(Side, ServerMessage)> {
        let messages = match command {
            Command::Claim { token, tx, reply } => self.claim(token, tx, reply),
            Command::Sync => self.state_messages(),
            Command::Message { side, msg } => self.handle_message(side, msg),
            Command::Disconnect { side, tx } => {
                let seat = &mut self.seats[seat_index(side)];
                if seat
                    .tx
                    .as_ref()
                    .is_some_and(|other| other.same_channel(&tx))
                {
                    seat.tx = None;
                }
                self.state_messages()
            }
        };
        for (side, message) in &messages {
            if let Some(tx) = self.tx_for(*side) {
                let _ = tx.send(message.clone());
            }
        }
        messages
    }

    pub fn tx_for(&self, side: Side) -> Option<Tx> {
        self.seats[seat_index(side)].tx.clone()
    }

    fn claim(
        &mut self,
        token: Option<String>,
        tx: Tx,
        reply: oneshot::Sender<ClaimOutcome>,
    ) -> Vec<(Side, ServerMessage)> {
        let reconnect_side = token
            .as_deref()
            .and_then(|wanted| self.side_with_token(wanted));
        let Some(side) = reconnect_side.or_else(|| self.free_side()) else {
            let _ = reply.send(ClaimOutcome::Full);
            return Vec::new();
        };

        let seat = &mut self.seats[seat_index(side)];
        let token = seat.token.clone().unwrap_or_else(generate_token);
        seat.token = Some(token.clone());
        seat.tx = Some(tx);

        if self.seats.iter().all(|seat| seat.token.is_some()) && self.game.is_none() {
            self.game = Some(new_game());
        }

        let _ = reply.send(ClaimOutcome::Seated { side, token });
        Vec::new()
    }

    fn handle_message(&mut self, side: Side, msg: ClientMessage) -> Vec<(Side, ServerMessage)> {
        match msg {
            ClientMessage::Hello { .. } => error(side, "already connected"),
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
                let both_claimed = self.seats.iter().all(|seat| seat.token.is_some());
                if !both_claimed || self.game.is_none() {
                    return error(side, "there is no game to leave");
                }
                if self.opponent_present(side) {
                    return error(side, "opponent is still connected");
                }

                let opponent = seat_index(side.other());
                self.seats[opponent] = Seat::default();
                self.game = None;
                self.state_messages()
            }
        }
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

    fn side_with_token(&self, wanted: &str) -> Option<Side> {
        [Side::Red, Side::Green].into_iter().find(|side| {
            self.seats[seat_index(*side)]
                .token
                .as_deref()
                .is_some_and(|token| token == wanted)
        })
    }

    fn free_side(&self) -> Option<Side> {
        [Side::Red, Side::Green]
            .into_iter()
            .find(|side| self.seats[seat_index(*side)].token.is_none())
    }

    fn opponent_present(&self, side: Side) -> bool {
        let opponent = &self.seats[seat_index(side.other())];
        opponent.token.is_some() && opponent.tx.is_some()
    }

    fn status(&self) -> Status {
        if self.game.is_none() || self.seats.iter().any(|seat| seat.token.is_none()) {
            return Status::Waiting;
        }
        if self
            .seats
            .iter()
            .any(|seat| seat.token.is_some() && seat.tx.is_none())
        {
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
                    .tx
                    .is_some()
                    .then_some((side, message))
            })
            .collect()
    }
}

fn error(side: Side, reason: &str) -> Vec<(Side, ServerMessage)> {
    vec![(
        side,
        ServerMessage::Error {
            reason: reason.to_owned(),
        },
    )]
}

pub async fn run(mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut session = Session::new();
    while let Some(command) = commands.recv().await {
        session.handle(command);
    }
}

#[cfg(test)]
mod tests {
    use hibedi_core::{Board, TokenId};

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

    fn claim(
        session: &mut Session,
        token: Option<&str>,
    ) -> (ClaimOutcome, Tx, mpsc::UnboundedReceiver<ServerMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply_tx, reply_rx) = oneshot::channel();
        session.handle(Command::Claim {
            token: token.map(str::to_owned),
            tx: tx.clone(),
            reply: reply_tx,
        });
        let outcome = reply_rx.blocking_recv().expect("claim reply");
        (outcome, tx, rx)
    }

    fn sync(session: &mut Session) -> Vec<(Side, ServerMessage)> {
        session.handle(Command::Sync)
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> Vec<ServerMessage> {
        let mut messages = Vec::new();
        while let Ok(message) = rx.try_recv() {
            messages.push(message);
        }
        messages
    }

    /// Both a plain `ServerMessage` and a `(Side, ServerMessage)` broadcast
    /// entry can be viewed as the message itself.
    trait AsMessage {
        fn as_message(&self) -> &ServerMessage;
    }

    impl AsMessage for ServerMessage {
        fn as_message(&self) -> &ServerMessage {
            self
        }
    }

    impl AsMessage for (Side, ServerMessage) {
        fn as_message(&self) -> &ServerMessage {
            &self.1
        }
    }

    fn last_state<M: AsMessage>(messages: &[M]) -> (Option<Game>, bool, Status) {
        messages
            .iter()
            .rev()
            .find_map(|message| match message.as_message() {
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

    fn err<M: AsMessage>(messages: &[M]) -> String {
        messages
            .iter()
            .find_map(|message| match message.as_message() {
                ServerMessage::Error { reason } => Some(reason.clone()),
                _ => None,
            })
            .expect("expected an error message")
    }

    fn token_of(outcome: &ClaimOutcome) -> String {
        match outcome {
            ClaimOutcome::Seated { token, .. } => token.clone(),
            ClaimOutcome::Full => panic!("expected a seat"),
        }
    }

    #[test]
    fn first_seat_is_red_second_is_green() {
        let mut session = Session::new();

        let (first, _, mut first_rx) = claim(&mut session, None);
        assert!(matches!(
            first,
            ClaimOutcome::Seated {
                side: Side::Red,
                ..
            }
        ));
        sync(&mut session);
        let (_, _, waiting) = last_state(&drain(&mut first_rx));
        assert_eq!(waiting, Status::Waiting);

        let (second, _, mut second_rx) = claim(&mut session, None);
        assert!(matches!(
            second,
            ClaimOutcome::Seated {
                side: Side::Green,
                ..
            }
        ));
        session.game = Some(live_game());
        sync(&mut session);

        let (first_game, _, first_status) = last_state(&drain(&mut first_rx));
        let (second_game, opponent_present, second_status) = last_state(&drain(&mut second_rx));
        assert!(first_game.is_some());
        assert_eq!(first_game, second_game);
        assert!(opponent_present);
        assert_eq!(first_status, Status::Playing);
        assert_eq!(second_status, Status::Playing);
    }

    #[test]
    fn third_player_is_rejected_while_both_seats_are_taken() {
        let mut session = Session::new();
        claim(&mut session, None);
        claim(&mut session, None);

        let (outcome, _, _) = claim(&mut session, None);
        assert_eq!(outcome, ClaimOutcome::Full);
    }

    #[test]
    fn unknown_token_gets_a_fresh_seat_when_one_is_free() {
        let mut session = Session::new();
        claim(&mut session, None);

        let (outcome, _, _) = claim(&mut session, Some("stale-token"));
        assert_eq!(
            outcome,
            ClaimOutcome::Seated {
                side: Side::Green,
                token: token_of(&outcome),
            }
        );
    }

    #[test]
    fn disconnect_pauses_and_token_reconnect_resumes() {
        let mut session = Session::new();
        let (first, red_tx, mut red_rx) = claim(&mut session, None);
        let red_token = token_of(&first);
        claim(&mut session, None);
        session.game = Some(live_game());
        sync(&mut session);
        drain(&mut red_rx);

        session.handle(Command::Disconnect {
            side: Side::Red,
            tx: red_tx,
        });

        // Red no longer has a seat connection, so only Green is notified.
        assert!(drain(&mut red_rx).is_empty());
        let green_rx = session.tx_for(Side::Green);
        assert!(green_rx.is_some());

        let (reconnected, _, mut red_rx) = claim(&mut session, Some(&red_token));
        assert_eq!(
            reconnected,
            ClaimOutcome::Seated {
                side: Side::Red,
                token: red_token
            }
        );
        sync(&mut session);

        let (_, opponent_present, status) = last_state(&drain(&mut red_rx));
        assert!(opponent_present);
        assert_eq!(status, Status::Playing);
    }

    #[test]
    fn moves_are_rejected_out_of_turn_and_accepted_in_turn() {
        let mut session = Session::new();
        claim(&mut session, None);
        let (_, _, mut green_rx) = claim(&mut session, None);
        session.game = Some(live_game());
        sync(&mut session);
        drain(&mut green_rx);

        // The live fixture makes red the starter.
        let reply = session.handle(Command::Message {
            side: Side::Green,
            msg: ClientMessage::Place { x: 0, y: 1 },
        });
        assert_eq!(err(&reply), "not your turn");

        // A jump before any token has been placed is an illegal action.
        let reply = session.handle(Command::Message {
            side: Side::Red,
            msg: ClientMessage::Jump {
                direction: hibedi_core::Direction::Up,
            },
        });
        assert_eq!(err(&reply), "illegal action");

        // Red's legal placement is accepted and broadcast to both seats.
        let reply = session.handle(Command::Message {
            side: Side::Red,
            msg: ClientMessage::Place { x: 2, y: 1 },
        });
        assert!(
            !reply
                .iter()
                .any(|(_, message)| matches!(message, ServerMessage::Error { .. }))
        );
        let (sent_to_green, _, _) = last_state(&drain(&mut green_rx));
        assert!(sent_to_green.is_some());
        assert_eq!(reply.len(), 2, "both seats receive the new state");
    }

    #[test]
    fn leave_frees_the_absent_opponents_seat_and_invalidates_their_token() {
        let mut session = Session::new();
        let (first, red_tx, _) = claim(&mut session, None);
        let red_token = token_of(&first);
        let (second, green_tx, _) = claim(&mut session, None);
        let green_token = token_of(&second);

        let paused = session.handle(Command::Disconnect {
            side: Side::Green,
            tx: green_tx,
        });
        let (_, opponent_present, status) = last_state(&paused);
        assert!(!opponent_present);
        assert_eq!(status, Status::Paused);

        let messages = session.handle(Command::Message {
            side: Side::Red,
            msg: ClientMessage::Leave,
        });
        let (_, _, status) = last_state(&messages);
        assert_eq!(status, Status::Waiting);
        assert!(session.game.is_none());

        // Red keeps their seat; Green's token no longer grants the old seat.
        let (reconnect, _, _) = claim(&mut session, Some(&red_token));
        assert!(matches!(
            reconnect,
            ClaimOutcome::Seated {
                side: Side::Red,
                ..
            }
        ));
        let (reclaim, _, _) = claim(&mut session, Some(&green_token));
        assert_eq!(
            reclaim,
            ClaimOutcome::Seated {
                side: Side::Green,
                token: token_of(&reclaim),
            }
        );

        // A second leave is refused while the opponent is connected.
        let reply = session.handle(Command::Message {
            side: Side::Red,
            msg: ClientMessage::Leave,
        });
        assert_eq!(err(&reply), "opponent is still connected");
        drop(red_tx);
    }

    #[test]
    fn new_game_requires_a_finished_game_and_a_connected_opponent() {
        let mut session = Session::new();
        claim(&mut session, None);
        claim(&mut session, None);

        session.game = Some(live_game());
        let reply = session.handle(Command::Message {
            side: Side::Red,
            msg: ClientMessage::NewGame,
        });
        assert_eq!(err(&reply), "the game is not over");

        let finished = finished_game();
        let finished_json = serde_json::to_string(&finished).unwrap();
        session.game = Some(finished.clone());

        let green_tx = session.tx_for(Side::Green).expect("green connected");
        session.handle(Command::Disconnect {
            side: Side::Green,
            tx: green_tx,
        });
        let reply = session.handle(Command::Message {
            side: Side::Red,
            msg: ClientMessage::NewGame,
        });
        assert_eq!(err(&reply), "opponent is not connected");

        let green_token = "green-token-reconnect";
        session.seats[1].token = Some(green_token.to_owned());
        claim(&mut session, Some(green_token));

        let reply = session.handle(Command::Message {
            side: Side::Red,
            msg: ClientMessage::NewGame,
        });
        let (game, _, status) = last_state(&reply);
        let game = game.expect("new game exists");
        assert_ne!(
            serde_json::to_string(&game).unwrap(),
            finished_json,
            "the finished game must be replaced"
        );
        assert_ne!(status, Status::Finished);
    }

    #[test]
    fn takeover_replaces_the_previous_connection_for_the_same_token() {
        let mut session = Session::new();
        let (first, red_tx, _) = claim(&mut session, None);
        let red_token = token_of(&first);
        claim(&mut session, None);
        session.game = Some(live_game());

        let (reconnected, new_red_tx, _) = claim(&mut session, Some(&red_token));
        assert!(matches!(
            reconnected,
            ClaimOutcome::Seated {
                side: Side::Red,
                ..
            }
        ));

        // The old connection's disconnect must not clear the new one.
        session.handle(Command::Disconnect {
            side: Side::Red,
            tx: red_tx,
        });
        assert!(
            session
                .tx_for(Side::Red)
                .is_some_and(|tx| tx.same_channel(&new_red_tx))
        );

        let (_, opponent_present, status) = last_state(&sync(&mut session));
        assert!(opponent_present);
        assert_eq!(status, Status::Playing);
    }
}
