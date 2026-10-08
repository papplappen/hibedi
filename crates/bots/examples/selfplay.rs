//! Prints a game the greedy bot plays against itself, in notation (see
//! `hibedi_core::notation`): handy for reaching a position to look at,
//! e.g. with `hibedi-client --game <file>`.
//!
//!     cargo run -p hibedi-bots --example selfplay -- [options] > game.txt
//!
//! (see `--help` for the options).

use clap::Parser;
use hibedi_bots::{Bot, Greedy};
use hibedi_core::{Action, BOARD_HEIGHT, BOARD_WIDTH, Game, notation};
use rand::SeedableRng;
use rand::rngs::StdRng;

/// Prints a game the greedy bot plays against itself, in notation.
#[derive(Parser)]
struct Args {
    /// Deal and play the same game every time
    #[arg(long)]
    seed: Option<u64>,
    /// The playable cells across
    #[arg(long, default_value_t = BOARD_WIDTH)]
    width: usize,
    /// The playable cells down
    #[arg(long, default_value_t = BOARD_HEIGHT)]
    height: usize,
    /// Stop after this many actions
    #[arg(long)]
    actions: Option<usize>,
    /// Stop when the turn after this many begins
    #[arg(long)]
    turns: Option<usize>,
}

fn main() {
    let args = Args::parse();
    let mut rng = match args.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let mut game = Game::new_random(args.width, args.height, &mut rng);
    let mut bot = Greedy;
    let mut turn = 0;
    while game.history().len() < args.actions.unwrap_or(usize::MAX) {
        let Some(action) = bot.choose(&game) else {
            break;
        };
        if matches!(action, Action::Start(_)) && game.places_token() {
            turn += 1;
            if turn > args.turns.unwrap_or(usize::MAX) {
                break;
            }
        }
        assert!(game.apply(action), "the bot takes listed actions");
    }
    print!("{}", notation::write(&game));
}
