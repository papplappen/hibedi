//! Playing by keyboard. Every action the player may take gets a key,
//! shown small in the corner of the cell it is aimed at.
//!
//! Hops go by direction: W A S D (also the arrow keys, and Vim's H J K
//! L). Tokens to start from are numbered in the game's order of actions
//! (row by row across the board), along the keyboard's rows: 1 to 0,
//! then Q to P, A to L and Z to M. The two never mix: while a token
//! hops, every action is a hop. Digits are read by position (so they
//! need no Shift on layouts like AZERTY), letters as the layout types
//! them, matching the labels.
//! Esc takes back the token chosen to jump, placed or activated, before
//! its first hop; F2 deals a new game, and so does Enter once a game is
//! over. Keys held with Ctrl, Alt or Super are left to the system.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use hibedi_core::{Action, Direction};

use super::{Overlay, Screen};
use crate::play::Match;
use crate::settings::Settings;

pub fn plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        play_by_keyboard
            .after(bevy::input::InputSystems)
            .run_if(in_state(Screen::Playing)),
    );
}

/// The keys actions are labeled with, in order.
const KEYS: [char; 36] = [
    '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', //
    'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', //
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', //
    'z', 'x', 'c', 'v', 'b', 'n', 'm',
];

/// Pairs actions with their keys: hops with their direction's, the
/// rest with [`KEYS`] in order. `TakeBack` has a key of its own (Esc)
/// and no cell to show a label on, so it is left out.
pub fn label(actions: &[Action]) -> impl Iterator<Item = (char, Action)> + '_ {
    let mut keys = KEYS.into_iter();
    actions.iter().filter_map(move |&action| match action {
        Action::Jump(direction) => Some((direction_key(direction), action)),
        Action::TakeBack => None,
        Action::Start(_) => keys.next().map(|key| (key, action)),
    })
}

/// The labeled key hopping in a direction.
const fn direction_key(direction: Direction) -> char {
    match direction {
        Direction::Up => 'w',
        Direction::Left => 'a',
        Direction::Down => 's',
        Direction::Right => 'd',
    }
}

/// The direction an unlabeled hop key stands for: arrows, and Vim's.
fn hop_alias(press: &KeyboardInput) -> Option<Direction> {
    match press.key_code {
        KeyCode::ArrowUp => Some(Direction::Up),
        KeyCode::ArrowLeft => Some(Direction::Left),
        KeyCode::ArrowDown => Some(Direction::Down),
        KeyCode::ArrowRight => Some(Direction::Right),
        _ => match typed(press)? {
            'k' => Some(Direction::Up),
            'h' => Some(Direction::Left),
            'j' => Some(Direction::Down),
            'l' => Some(Direction::Right),
            _ => None,
        },
    }
}

fn play_by_keyboard(
    mut presses: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    overlay: Res<State<Overlay>>,
    settings: Res<Settings>,
    mut play: ResMut<Match>,
) {
    // Keys pressed while something covers the board are not for it
    // (and are read here, so they do not reach it later).
    if *overlay.get() != Overlay::None {
        presses.clear();
        return;
    }
    // Keys with Ctrl, Alt or Super are shortcuts of the system or the
    // browser, not the game's.
    let shortcut = held.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    for press in presses.read() {
        if press.state != ButtonState::Pressed || press.repeat || shortcut {
            continue;
        }
        let over = play.game().is_some_and(|game| game.result().is_some());
        let action = match press.key_code {
            KeyCode::F2 => {
                play.start_new_game(settings.setup());
                continue;
            }
            KeyCode::Enter | KeyCode::NumpadEnter if over => {
                play.start_new_game(settings.setup());
                continue;
            }
            KeyCode::Escape => Some(Action::TakeBack),
            _ => {
                let actions = play.actions();
                hop_alias(press)
                    .map(Action::Jump)
                    .filter(|hop| actions.contains(hop))
                    .or_else(|| {
                        let key = typed(press)?;
                        label(&actions)
                            .find(|&(label, _)| label == key)
                            .map(|(_, action)| action)
                    })
            }
        };
        // Only a possible action marks the match changed.
        if let Some(action) = action
            && play.actions().contains(&action)
        {
            play.act(action);
        }
    }
}

/// The key label a press stands for, if any.
fn typed(press: &KeyboardInput) -> Option<char> {
    let digit = match press.key_code {
        KeyCode::Digit0 | KeyCode::Numpad0 => Some('0'),
        KeyCode::Digit1 | KeyCode::Numpad1 => Some('1'),
        KeyCode::Digit2 | KeyCode::Numpad2 => Some('2'),
        KeyCode::Digit3 | KeyCode::Numpad3 => Some('3'),
        KeyCode::Digit4 | KeyCode::Numpad4 => Some('4'),
        KeyCode::Digit5 | KeyCode::Numpad5 => Some('5'),
        KeyCode::Digit6 | KeyCode::Numpad6 => Some('6'),
        KeyCode::Digit7 | KeyCode::Numpad7 => Some('7'),
        KeyCode::Digit8 | KeyCode::Numpad8 => Some('8'),
        KeyCode::Digit9 | KeyCode::Numpad9 => Some('9'),
        _ => None,
    };
    digit.or_else(|| match &press.logical_key {
        Key::Character(text) => {
            let mut chars = text.chars().flat_map(char::to_lowercase);
            let key = chars.next()?;
            chars.next().is_none().then_some(key)
        }
        _ => None,
    })
}
