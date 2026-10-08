//! Everything on screen, built with Bevy UI, and turning clicks into
//! moves. The screen follows the [`Match`] resource: views redraw when
//! it changes, animating the difference.
//!
//! - [`layout`]: the skeleton, adapting to the window,
//! - [`board`]: cells and tokens,
//! - [`hud`]: everything around the board,
//! - [`keys`]: playing by keyboard,
//! - [`anim`]: the animations views use,
//! - [`tap`]: taps and clicks on nodes,
//! - [`theme`]: colors and fonts.

mod anim;
mod board;
mod hud;
mod keys;
mod layout;
mod tap;
mod theme;

use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
pub use hud::Notice;

use crate::play::Match;

/// Whether there is a game to show yet. Online, there may not be: while
/// a relay seats the players, or until the first seat's board arrives.
#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    /// A notice says what we are waiting for.
    #[default]
    Waiting,
    /// The board and the sidebar.
    Playing,
}

pub fn plugin(app: &mut App) {
    app.init_state::<Screen>()
        .add_plugins((
            theme::plugin,
            anim::plugin,
            layout::plugin,
            board::plugin,
            hud::plugin,
            keys::plugin,
            tap::plugin,
        ))
        .add_systems(Startup, (spawn_camera, layout::spawn, hud::spawn).chain())
        .add_systems(
            OnEnter(Screen::Playing),
            (board::spawn, hud::spawn_sidebar).chain(),
        )
        .add_systems(Update, start_playing.run_if(in_state(Screen::Waiting)));
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn start_playing(play: Option<Res<Match>>, mut screen: ResMut<NextState<Screen>>) {
    if play.is_some_and(|play| play.game().is_some()) {
        screen.set(Screen::Playing);
    }
}
