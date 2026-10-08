//! The player's settings, kept on the device (see [`crate::storage`])
//! and saved whenever they change. Plain data: what each setting does
//! is up to the parts of the game reading it.

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
    /// The boards dealt, from the next game on.
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

impl Settings {
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

/// Which animations play; those switched off jump to their end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Motion {
    /// Tokens hop from cell to cell (else they appear where they land).
    pub hops: bool,
    /// Cells turn over as they change color.
    pub flips: bool,
    /// Things grow in and out, counts swell as they change.
    pub pops: bool,
    /// Hints and glowing tokens breathe.
    pub pulses: bool,
    /// The winner's cells jump for joy.
    pub cheers: bool,
    /// Colors fade into each other.
    pub fades: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            hops: true,
            flips: true,
            pops: true,
            pulses: true,
            cheers: true,
            fades: true,
        }
    }
}

/// The look of the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
