//! Prints a game the greedy bot plays against itself, in notation (see
//! `hibedi_core::notation`): handy for reaching a position to look at,
//! e.g. with `hibedi-client --game <file>`.
//!
//!     cargo run -p hibedi-bots --example selfplay -- [options] > game.txt
//!
//!   --seed <n>      deal and play the same game every time
//!   --size <w>x<h>  the playable cells (default 4x5)
//!   --actions <n>   stop after this many actions (default: play it out)
//!   --turns <n>     stop when the turn after this many begins

use hibedi_bots::{Bot, Greedy};
use hibedi_core::{Action, BOARD_HEIGHT, BOARD_WIDTH, Game, notation};
use rand::SeedableRng;
use rand::rngs::StdRng;

fn main() {
    let mut seed = None;
    let (mut width, mut height) = (BOARD_WIDTH, BOARD_HEIGHT);
    let mut actions = usize::MAX;
    let mut turns = usize::MAX;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .unwrap_or_else(|| usage(&format!("{flag} needs a value")));
        let number = |text: &str| -> usize {
            text.parse()
                .unwrap_or_else(|_| usage(&format!("not a number: {text}")))
        };
        match flag.as_str() {
            "--seed" => seed = Some(number(&value) as u64),
            "--size" => {
                let (w, h) = value
                    .split_once('x')
                    .unwrap_or_else(|| usage("--size takes <w>x<h>"));
                (width, height) = (number(w), number(h));
            }
            "--actions" => actions = number(&value),
            "--turns" => turns = number(&value),
            other => usage(&format!("unknown argument: {other}")),
        }
    }

    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let mut game = Game::new_random(width, height, &mut rng);
    let mut bot = Greedy;
    let mut turn = 0;
    while game.history().len() < actions {
        let Some(action) = bot.choose(&game) else {
            break;
        };
        if matches!(action, Action::Start(_)) && game.places_token() {
            turn += 1;
            if turn > turns {
                break;
            }
        }
        assert!(game.apply(action), "the bot takes listed actions");
    }
    print!("{}", notation::write(&game));
}

fn usage(problem: &str) -> ! {
    eprintln!("{problem}");
    eprintln!("usage: selfplay [--seed <n>] [--size <w>x<h>] [--actions <n>] [--turns <n>]");
    std::process::exit(2);
}
