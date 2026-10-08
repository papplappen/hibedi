//! The player's settings, kept on the device (see [`crate::storage`])
//! and saved whenever the player changes them. Plain data: what each
//! setting does is up to the parts of the game reading it.
//!
//! Natively, every setting can also be given on the command line
//! ([`SettingsArgs`]), for that run: it is saved only along with a later
//! change.

use std::collections::BTreeSet;

use bevy::prelude::*;
use hibedi_bots::BotKind;
use hibedi_core::{BOARD_HEIGHT, BOARD_WIDTH, Side};
use serde::{Deserialize, Serialize};

use crate::storage;

const KEY: &str = "settings";

pub fn plugin(app: &mut App) {
    app.add_systems(Last, save.run_if(resource_changed::<Settings>));
}

#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The boards dealt.
    pub board: BoardSize,
    /// Who plays the other side in games on this device.
    pub opponent: Opponent,
    /// The side taken against a bot.
    pub side: Side,
    pub motion: Motion,
    pub theme: ThemeKind,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            board: BoardSize::DEFAULT,
            opponent: Opponent::Human,
            side: Side::Red,
            motion: Motion::default(),
            theme: ThemeKind::Classic,
        }
    }
}

/// What a game is dealt with: the board, and who plays it. A game keeps
/// the setup it was dealt with; a new setup takes a new game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setup {
    pub board: BoardSize,
    pub opponent: Opponent,
    pub side: Side,
}

impl Settings {
    pub fn setup(&self) -> Setup {
        Setup {
            board: self.board,
            opponent: self.opponent,
            side: self.side,
        }
    }

    /// The settings kept on the device; the defaults for anything
    /// missing or unreadable.
    pub fn load() -> Self {
        let mut settings: Self = storage::load(KEY)
            .and_then(|text| {
                serde_json::from_str(&text)
                    .inspect_err(|error| warn!("ignoring unreadable settings: {error}"))
                    .ok()
            })
            .unwrap_or_default();
        if !BoardSize::ALL.contains(&settings.board) {
            settings.board = BoardSize::DEFAULT;
        }
        settings
    }
}

fn save(settings: Res<Settings>) {
    // Not as loaded (or as given on the command line), only as changed.
    if settings.is_added() {
        return;
    }
    let text = serde_json::to_string(&*settings).expect("settings always serialize");
    storage::save(KEY, &text);
}

/// The playable cells across and down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardSize {
    pub width: usize,
    pub height: usize,
}

impl BoardSize {
    pub const DEFAULT: Self = Self::new(BOARD_WIDTH, BOARD_HEIGHT);

    /// The sizes on offer: the default, and up to two cells smaller or
    /// larger each way.
    pub const ALL: [Self; 5] = [
        Self::new(BOARD_WIDTH - 2, BOARD_HEIGHT - 2),
        Self::new(BOARD_WIDTH - 1, BOARD_HEIGHT - 1),
        Self::DEFAULT,
        Self::new(BOARD_WIDTH + 1, BOARD_HEIGHT + 1),
        Self::new(BOARD_WIDTH + 2, BOARD_HEIGHT + 2),
    ];

    const fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Opponent {
    /// Another player on this device, taking turns.
    Human,
    Bot(BotKind),
}

/// A kind of animation, which the player may switch off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(clap::ValueEnum))]
#[serde(rename_all = "snake_case")]
pub enum Animation {
    /// Tokens hop from cell to cell (else they appear where they land).
    Hops,
    /// Cells turn over as they change color.
    Flips,
    /// Things grow in and out, counts swell as they change.
    Pops,
    /// Hints and glowing tokens breathe.
    Pulses,
    /// The winner's cells jump for joy.
    Cheers,
    /// Colors fade into each other.
    Fades,
}

impl Animation {
    pub const ALL: [Self; 6] = [
        Self::Hops,
        Self::Flips,
        Self::Pops,
        Self::Pulses,
        Self::Cheers,
        Self::Fades,
    ];

    /// What players read in the settings.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hops => "Tokens hop",
            Self::Flips => "Cells flip over",
            Self::Pops => "Things pop in and out",
            Self::Pulses => "Hints pulse",
            Self::Cheers => "Winning cells cheer",
            Self::Fades => "Colors fade",
        }
    }
}

/// Which animations play; those switched off jump to their end.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    off: BTreeSet<Animation>,
}

impl Motion {
    pub fn plays(&self, animation: Animation) -> bool {
        !self.off.contains(&animation)
    }

    pub fn set(&mut self, animation: Animation, on: bool) {
        if on {
            self.off.remove(&animation);
        } else {
            self.off.insert(animation);
        }
    }
}

/// The look of the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(clap::ValueEnum))]
#[serde(rename_all = "snake_case")]
pub enum ThemeKind {
    /// Flat colored tiles and flat tokens.
    Classic,
    /// Playing cards with hearts and clubs, and stones shaded like Go
    /// stones.
    Cards,
}

impl ThemeKind {
    pub const ALL: [Self; 2] = [Self::Classic, Self::Cards];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Cards => "Cards",
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use command_line::SettingsArgs;

/// The settings as command line options (native only).
#[cfg(not(target_arch = "wasm32"))]
mod command_line {
    use std::sync::LazyLock;

    use clap::ValueEnum;
    use clap::builder::PossibleValue;

    use super::*;

    /// Settings for this run; the rest stay as saved.
    #[derive(clap::Args, Debug, Default)]
    pub struct SettingsArgs {
        /// Who plays the other side on this device
        #[arg(long)]
        opponent: Option<Opponent>,
        /// The side you take against a bot
        #[arg(long)]
        side: Option<Side>,
        /// The boards dealt
        #[arg(long)]
        size: Option<BoardSize>,
        /// The board's look
        #[arg(long)]
        theme: Option<ThemeKind>,
        /// Play only these animations
        #[arg(long, value_delimiter = ',', value_name = "KINDS")]
        animations: Option<Vec<Animation>>,
        /// Play no animations
        #[arg(long, conflicts_with = "animations")]
        no_animations: bool,
    }

    impl SettingsArgs {
        pub fn apply(self, settings: &mut Settings) {
            if let Some(opponent) = self.opponent {
                settings.opponent = opponent;
            }
            if let Some(side) = self.side {
                settings.side = side;
            }
            if let Some(size) = self.size {
                settings.board = size;
            }
            if let Some(theme) = self.theme {
                settings.theme = theme;
            }
            if let Some(animations) = self.animations {
                settings.motion = Motion::only(animations);
            }
            if self.no_animations {
                settings.motion = Motion::only([]);
            }
        }
    }

    impl Motion {
        /// Only the animations given play.
        pub fn only(on: impl IntoIterator<Item = Animation>) -> Self {
            let on: BTreeSet<_> = on.into_iter().collect();
            Self {
                off: Animation::ALL
                    .into_iter()
                    .filter(|animation| !on.contains(animation))
                    .collect(),
            }
        }
    }

    /// `human`, or a bot by its name.
    impl ValueEnum for Opponent {
        fn value_variants<'a>() -> &'a [Self] {
            static ALL: LazyLock<Vec<Opponent>> = LazyLock::new(|| {
                std::iter::once(Opponent::Human)
                    .chain(BotKind::ALL.map(Opponent::Bot))
                    .collect()
            });
            &ALL
        }

        fn to_possible_value(&self) -> Option<PossibleValue> {
            match self {
                Self::Human => Some(PossibleValue::new("human").help("Someone else here")),
                Self::Bot(bot) => bot.to_possible_value(),
            }
        }
    }

    /// `<width>x<height>`.
    impl ValueEnum for BoardSize {
        fn value_variants<'a>() -> &'a [Self] {
            &Self::ALL
        }

        fn to_possible_value(&self) -> Option<PossibleValue> {
            Some(PossibleValue::new(format!(
                "{}x{}",
                self.width, self.height
            )))
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        settings: SettingsArgs,
    }

    fn apply(args: &[&str]) -> Result<Settings, clap::Error> {
        let cli = Cli::try_parse_from(std::iter::once("test").chain(args.iter().copied()))?;
        let mut settings = Settings::default();
        cli.settings.apply(&mut settings);
        Ok(settings)
    }

    #[test]
    fn options_set_what_they_name() {
        let settings = apply(&[
            "--opponent=greedy",
            "--side=green",
            "--size=6x7",
            "--theme=cards",
            "--animations=hops,fades",
        ])
        .unwrap();
        assert_eq!(settings.opponent, Opponent::Bot(BotKind::Greedy));
        assert_eq!(settings.side, Side::Green);
        assert_eq!(settings.board, BoardSize::ALL[4]);
        assert_eq!(settings.theme, ThemeKind::Cards);
        assert_eq!(
            Animation::ALL.map(|animation| settings.motion.plays(animation)),
            [true, false, false, false, false, true]
        );

        let quiet = apply(&["--no-animations"]).unwrap();
        assert!(
            Animation::ALL
                .iter()
                .all(|&animation| !quiet.motion.plays(animation))
        );
        assert_eq!(apply(&[]).unwrap(), Settings::default());
    }

    #[test]
    fn unknown_values_are_refused() {
        assert!(apply(&["--size=9x9"]).is_err());
        assert!(apply(&["--animations=hops,spins"]).is_err());
        assert!(apply(&["--opponent=nobody"]).is_err());
        assert!(apply(&["--animations=hops", "--no-animations"]).is_err());
    }
}
