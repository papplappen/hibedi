//! Small animations for UI nodes. Each is a component driving one
//! aspect of its node; one-shot animations remove themselves when done.
//! Views only insert them: inserting one again restarts it.
//!
//! | component   | drives                              |
//! |-------------|-------------------------------------|
//! | [`Flip`]    | horizontal scale, background        |
//! | [`Hop`]     | `left`/`top`, scale                 |
//! | [`ScaleTo`] | scale                               |
//! | [`Bounce`]  | vertical translation                |
//! | [`Pulse`]   | scale (looping)                     |
//! | [`ColorTo`] | background (eased toward a target)  |

use std::f32::consts::{PI, TAU};

use bevy::color::color_difference::EuclideanDistance;
use bevy::prelude::*;

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

/// Up and back down over the progress `t`: 0 at both ends, 1 halfway.
fn arc(t: f32) -> f32 {
    (PI * t).sin()
}

/// Turns a node over like a card, showing the new color from halfway.
#[derive(Component)]
pub struct Flip {
    clock: Clock,
    to: Color,
}

impl Flip {
    const DURATION: f32 = 0.32;

    pub const fn to(color: Color, delay: f32) -> Self {
        Self {
            clock: Clock {
                delay,
                ..Clock::new(Self::DURATION)
            },
            to: color,
        }
    }
}

fn flip(
    time: Res<Time>,
    mut commands: Commands,
    mut nodes: Query<(Entity, &mut Flip, &mut UiTransform, &mut BackgroundColor)>,
) {
    for (entity, mut flip, mut transform, mut background) in &mut nodes {
        let Some(t) = flip.clock.tick(time.delta_secs()) else {
            continue;
        };
        transform.scale = vec2((PI * t).cos().abs(), 1.0 + 0.08 * arc(t));
        if t >= 0.5 {
            background.0 = flip.to;
        }
        if t >= 1.0 {
            transform.scale = Vec2::ONE;
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
    mut commands: Commands,
    mut nodes: Query<(Entity, &mut Hop, &mut Node, &mut UiTransform)>,
) {
    for (entity, mut hop, mut node, mut transform) in &mut nodes {
        let Some(t) = hop.clock.tick(time.delta_secs()) else {
            continue;
        };
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
    mut commands: Commands,
    mut nodes: Query<(Entity, &mut ScaleTo, &mut UiTransform)>,
) {
    for (entity, mut scale, mut transform) in &mut nodes {
        // Before a delayed start the node already shows its first frame.
        let t = scale.clock.tick(time.delta_secs()).unwrap_or(0.0);
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
    mut commands: Commands,
    mut nodes: Query<(Entity, &mut Bounce, &mut UiTransform)>,
) {
    for (entity, mut bounce, mut transform) in &mut nodes {
        let Some(t) = bounce.clock.tick(time.delta_secs()) else {
            continue;
        };
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

fn pulse(time: Res<Time>, mut nodes: Query<(&Pulse, &mut UiTransform)>) {
    let phase = (TAU * time.elapsed_secs() / Pulse::PERIOD).sin();
    for (pulse, mut transform) in &mut nodes {
        transform.scale = Vec2::splat(1.0 + pulse.amount * phase);
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

fn color_to(time: Res<Time>, mut nodes: Query<(&ColorTo, &mut BackgroundColor)>) {
    let step = 1.0 - (-ColorTo::RATE * time.delta_secs()).exp();
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
