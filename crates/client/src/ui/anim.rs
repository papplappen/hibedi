//! Small animations for UI nodes. Each is a component driving one
//! aspect of its node; one-shot animations remove themselves when done.
//! Views only insert them: inserting one again restarts it.
//!
//! The player may switch each kind off in the settings ([`crate::settings::Motion`]):
//! then it jumps to its end, though a flip still waits for its moment.
//!
//! | component   | drives                              |
//! |-------------|-------------------------------------|
//! | [`Flip`]    | scale, translation, [`Face`]        |
//! | [`Hop`]     | `left`/`top`, scale                 |
//! | [`ScaleTo`] | scale                               |
//! | [`Bounce`]  | vertical translation                |
//! | [`Pulse`]   | scale (looping)                     |
//! | [`ColorTo`] | background (eased toward a target)  |

use std::f32::consts::{PI, TAU};

use bevy::color::color_difference::EuclideanDistance;
use bevy::prelude::*;

use super::theme::Face;
use crate::settings::Settings;

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (flip, hop, scale_to, bounce, pulse, color_to).in_set(Animate),
    );
}

/// The animation systems; views schedule their changes before them.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Animate;

/// Progress through a one-shot animation that starts after a delay.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    delay: f32,
    duration: f32,
    elapsed: f32,
}

impl Clock {
    pub const fn new(duration: f32) -> Self {
        Self {
            delay: 0.0,
            duration,
            elapsed: 0.0,
        }
    }

    /// Advances by `dt` seconds; returns the progress (0 to 1) once
    /// started.
    fn tick(&mut self, dt: f32) -> Option<f32> {
        self.elapsed += dt;
        let running = self.elapsed - self.delay;
        (running >= 0.0).then(|| (running / self.duration).min(1.0))
    }
}

/// The progress shown: `t`, or the end for animations switched off.
fn shown(t: f32, on: bool) -> f32 {
    if on { t } else { 1.0 }
}

/// Up and back down over the progress `t`: 0 at both ends, 1 halfway.
fn arc(t: f32) -> f32 {
    (PI * t).sin()
}

/// Turns a node over like a card, showing the new face from halfway.
/// Given a direction, the turn runs along it, as if swept over by
/// something passing: the old face folds away into the front edge, and
/// the new one unfolds from the back edge.
#[derive(Component)]
pub struct Flip {
    clock: Clock,
    to: Face,
    /// Whether the new face shows yet.
    turned: bool,
    /// A unit step in screen space (y down), or zero to turn in place.
    direction: Vec2,
}

impl Flip {
    const DURATION: f32 = 0.32;
    /// How much the node grows across the turn halfway through.
    const LIFT: f32 = 0.08;

    pub const fn to(face: Face, delay: f32, direction: Vec2) -> Self {
        Self {
            clock: Clock {
                delay,
                ..Clock::new(Self::DURATION)
            },
            to: face,
            turned: false,
            direction,
        }
    }
}

/// The node showing the symbol of a face's node (see [`show_face`]).
#[derive(Component)]
pub struct FaceSymbol(pub Entity);

/// Shows a face on a node at once: its color, and its symbol on the
/// node's [`FaceSymbol`].
pub fn show_face(commands: &mut Commands, symbol: Entity, face: &Face) -> BackgroundColor {
    let mut node = commands.entity(symbol);
    match &face.symbol {
        Some((image, tint)) => {
            node.insert((
                ImageNode::new(image.clone()).with_color(*tint),
                Visibility::Inherited,
            ));
        }
        None => {
            node.insert(Visibility::Hidden);
        }
    }
    BackgroundColor(face.color)
}

fn flip(
    time: Res<Time>,
    settings: Res<Settings>,
    mut commands: Commands,
    mut nodes: Query<(
        Entity,
        &mut Flip,
        &mut UiTransform,
        &mut BackgroundColor,
        &FaceSymbol,
    )>,
) {
    for (entity, mut flip, mut transform, mut background, &FaceSymbol(symbol)) in &mut nodes {
        let Some(t) = flip.clock.tick(time.delta_secs()) else {
            continue;
        };
        let t = shown(t, settings.motion.flips);
        // Turning in place, the node turns about its vertical axis.
        let along = if flip.direction.y.abs() > flip.direction.x.abs() {
            Vec2::Y
        } else {
            Vec2::X
        };
        let width = (PI * t).cos().abs();
        transform.scale = along * width + (Vec2::ONE - along) * (1.0 + Flip::LIFT * arc(t));
        // The narrowed face hugs the front edge, then the back edge.
        let hug = if t < 0.5 { 1.0 } else { -1.0 };
        let shift = flip.direction * hug * (1.0 - width) / 2.0 * 100.0;
        transform.translation = Val2::percent(shift.x, shift.y);
        if t >= 0.5 && !flip.turned {
            flip.turned = true;
            *background = show_face(&mut commands, symbol, &flip.to);
        }
        if t >= 1.0 {
            transform.scale = Vec2::ONE;
            transform.translation = Val2::ZERO;
            commands.entity(entity).remove::<Flip>();
        }
    }
}

/// Jumps an absolutely positioned node from one `left`/`top` position
/// (in percent) to another, growing as it lifts off the board.
#[derive(Component)]
pub struct Hop {
    clock: Clock,
    from: Vec2,
    to: Vec2,
}

impl Hop {
    /// How much larger a hopping node grows at the top of its jump.
    const LIFT: f32 = 0.35;

    pub const fn new(from: Vec2, to: Vec2, duration: f32) -> Self {
        Self {
            clock: Clock::new(duration),
            from,
            to,
        }
    }
}

fn hop(
    time: Res<Time>,
    settings: Res<Settings>,
    mut commands: Commands,
    mut nodes: Query<(Entity, &mut Hop, &mut Node, &mut UiTransform)>,
) {
    for (entity, mut hop, mut node, mut transform) in &mut nodes {
        let Some(t) = hop.clock.tick(time.delta_secs()) else {
            continue;
        };
        let t = shown(t, settings.motion.hops);
        let at = hop
            .from
            .lerp(hop.to, EaseFunction::SineInOut.sample_clamped(t));
        node.left = percent(at.x);
        node.top = percent(at.y);
        transform.scale = Vec2::splat(1.0 + Hop::LIFT * arc(t));
        if t >= 1.0 {
            commands.entity(entity).remove::<Hop>();
        }
    }
}

/// Scales a node from one size to another, optionally despawning it at
/// the end.
#[derive(Component)]
pub struct ScaleTo {
    clock: Clock,
    from: f32,
    to: f32,
    ease: EaseFunction,
    despawn: bool,
}

impl ScaleTo {
    /// Grows a new node into place, overshooting a little.
    pub const fn appear(delay: f32) -> Self {
        Self {
            clock: Clock {
                delay,
                ..Clock::new(0.35)
            },
            from: 0.0,
            to: 1.0,
            ease: EaseFunction::BackOut,
            despawn: false,
        }
    }

    /// Shrinks a node away, then despawns it.
    pub const fn disappear() -> Self {
        Self {
            clock: Clock::new(0.2),
            from: 1.0,
            to: 0.0,
            ease: EaseFunction::QuadraticIn,
            despawn: true,
        }
    }

    /// A short swell drawing the eye to a change.
    pub const fn bump() -> Self {
        Self {
            clock: Clock::new(0.3),
            from: 1.3,
            to: 1.0,
            ease: EaseFunction::QuadraticOut,
            despawn: false,
        }
    }
}

fn scale_to(
    time: Res<Time>,
    settings: Res<Settings>,
    mut commands: Commands,
    mut nodes: Query<(Entity, &mut ScaleTo, &mut UiTransform)>,
) {
    for (entity, mut scale, mut transform) in &mut nodes {
        // Before a delayed start the node already shows its first frame.
        let t = scale.clock.tick(time.delta_secs()).unwrap_or(0.0);
        let t = shown(t, settings.motion.pops);
        let eased = scale.ease.sample_clamped(t);
        transform.scale = Vec2::splat(scale.from + (scale.to - scale.from) * eased);
        if t >= 1.0 {
            if scale.despawn {
                commands.entity(entity).despawn();
            } else {
                commands.entity(entity).remove::<ScaleTo>();
            }
        }
    }
}

/// Bounces a node up and back down once.
#[derive(Component)]
pub struct Bounce {
    clock: Clock,
}

impl Bounce {
    /// Height of the bounce, in percent of the node's height.
    const HEIGHT: f32 = 22.0;

    pub const fn after(delay: f32) -> Self {
        Self {
            clock: Clock {
                delay,
                ..Clock::new(0.45)
            },
        }
    }
}

fn bounce(
    time: Res<Time>,
    settings: Res<Settings>,
    mut commands: Commands,
    mut nodes: Query<(Entity, &mut Bounce, &mut UiTransform)>,
) {
    for (entity, mut bounce, mut transform) in &mut nodes {
        let Some(t) = bounce.clock.tick(time.delta_secs()) else {
            continue;
        };
        let t = shown(t, settings.motion.cheers);
        transform.translation.y = percent(-Bounce::HEIGHT * arc(t));
        if t >= 1.0 {
            commands.entity(entity).remove::<Bounce>();
        }
    }
}

/// Breathes a node's scale around its size, forever. Every pulse shares
/// one rhythm, so hints on the board breathe together.
#[derive(Component)]
pub struct Pulse {
    /// Largest deviation from the node's size, as a fraction of it.
    pub amount: f32,
}

impl Pulse {
    const PERIOD: f32 = 1.4;
}

fn pulse(time: Res<Time>, settings: Res<Settings>, mut nodes: Query<(&Pulse, &mut UiTransform)>) {
    let phase = if settings.motion.pulses {
        (TAU * time.elapsed_secs() / Pulse::PERIOD).sin()
    } else {
        0.0
    };
    for (pulse, mut transform) in &mut nodes {
        let scale = Vec2::splat(1.0 + pulse.amount * phase);
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// Eases a node's background toward a color; changing the target
/// continues from the color shown.
#[derive(Component)]
pub struct ColorTo(pub Color);

impl ColorTo {
    /// How quickly the color follows, per second.
    const RATE: f32 = 14.0;
}

fn color_to(
    time: Res<Time>,
    settings: Res<Settings>,
    mut nodes: Query<(&ColorTo, &mut BackgroundColor)>,
) {
    let step = if settings.motion.fades {
        1.0 - (-ColorTo::RATE * time.delta_secs()).exp()
    } else {
        1.0
    };
    for (target, mut background) in &mut nodes {
        let current = background.0.to_linear();
        let target = target.0.to_linear();
        let next = if current.distance(&target) < 0.002 {
            target
        } else {
            current.mix(&target, step)
        };
        background.set_if_neq(BackgroundColor(next.into()));
    }
}
