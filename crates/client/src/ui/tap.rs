//! Taps and clicks on UI nodes, read straight from the mouse and touch
//! input: a [`Tap`] is triggered on the [`Tappable`] node under a
//! released mouse button or lifted finger.
//!
//! Bevy's picking only clicks a node the pointer hovered in an earlier
//! frame, so it loses a tap, or a quick click, that starts and ends
//! within one frame. On touch screens that is most taps.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, detect_taps.after(bevy::input::InputSystems));
}

/// Marks a node that receives [`Tap`]s.
#[derive(Component)]
pub struct Tappable;

/// A tap or click on a [`Tappable`] node.
#[derive(EntityEvent)]
pub struct Tap {
    pub entity: Entity,
}

fn detect_taps(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window, With<PrimaryWindow>>,
    nodes: Query<
        (
            Entity,
            &ComputedNode,
            &UiGlobalTransform,
            &InheritedVisibility,
        ),
        With<Tappable>,
    >,
) {
    // UI nodes are laid out in physical pixels.
    let clicked = mouse
        .just_released(MouseButton::Left)
        .then(|| window.physical_cursor_position())
        .flatten();
    let tapped = touches
        .iter_just_released()
        .map(|touch| touch.position() * window.scale_factor());

    for point in clicked.into_iter().chain(tapped) {
        let hit = nodes.iter().find(|(_, node, transform, visibility)| {
            visibility.get() && node.contains_point(**transform, point)
        });
        if let Some((entity, ..)) = hit {
            commands.trigger(Tap { entity });
        }
    }
}
