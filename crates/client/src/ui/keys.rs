//! Playing by keyboard. Every action the player may take gets a key,
//! shown small in the corner of the cell it is aimed at; keys follow the
//! game's order of actions (row by row across the board).
//!
//! Keys run along the keyboard's rows: 1 to 0, then Q to P, A to L and
//! Z to M. Digits are read by position (so they need no Shift on layouts
//! like AZERTY), letters as the layout types them, matching the labels.
//! Esc takes back the token chosen to jump, placed or activated, before
//! its first hop; F2 deals a new game, and so does Enter once a game is
//! over.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use hibedi_core::Action;

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

/// Pairs actions with their keys. `TakeBack` has a key of its own (Esc)
/// and no cell to show a label on, so it is left out.
pub fn label(actions: &[Action]) -> impl Iterator<Item = (char, Action)> + '_ {
    KEYS.into_iter().zip(
        actions
            .iter()
            .copied()
            .filter(|&action| action != Action::TakeBack),
    )
}

fn play_by_keyboard(
    mut presses: MessageReader<KeyboardInput>,
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
    for press in presses.read() {
        if press.state != ButtonState::Pressed || press.repeat {
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
            _ => typed(press)
                .and_then(|key| label(&play.actions()).find(|&(label, _)| label == key))
                .map(|(_, action)| action),
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
