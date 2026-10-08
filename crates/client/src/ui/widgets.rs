//! The few widgets the screen needs, built on [`super::tap`]: buttons,
//! and controls bound to a setting.
//!
//! Bevy's own widgets (`bevy_ui_widgets`) are not used: they act on
//! picking events, which lose quick taps (see [`super::tap`]).

use bevy::prelude::*;

use super::anim::{Animate, ColorTo};
use super::tap::{Tap, Tappable};
use super::theme::{Fonts, TextStyle, palette};
use crate::settings::Settings;

pub fn plugin(app: &mut App) {
    app.add_observer(tap_control).add_systems(
        Update,
        (
            hover_buttons,
            show_controls
                .run_if(resource_changed::<Settings>.or_eager(any_match_filter::<Added<Control>>)),
        )
            .before(Animate),
    );
}

/// A button's background at rest and under the pointer.
#[derive(Component, PartialEq)]
pub struct ButtonColors {
    pub rest: Color,
    pub hover: Color,
}

impl ButtonColors {
    pub fn of(color: Color) -> Self {
        Self {
            rest: color,
            hover: color.lighter(0.08),
        }
    }
}

/// A button labeled `label`, laid out by `node`; observe [`super::tap::Tap`]
/// on it.
pub fn button(
    fonts: &Fonts,
    label: &str,
    text: TextStyle,
    color: Color,
    node: Node,
) -> impl Bundle {
    (
        Button,
        Tappable,
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(10)),
            ..node
        },
        BackgroundColor(color),
        ButtonColors::of(color),
        UiTransform::IDENTITY,
        children![(ButtonLabel, fonts.text(label, text))],
    )
}

/// A button's text.
#[derive(Component)]
pub struct ButtonLabel;

fn hover_buttons(
    mut commands: Commands,
    buttons: Query<
        (Entity, &Interaction, &ButtonColors),
        Or<(Changed<Interaction>, Changed<ButtonColors>)>,
    >,
) {
    for (entity, interaction, colors) in &buttons {
        let color = match interaction {
            Interaction::None => colors.rest,
            Interaction::Hovered | Interaction::Pressed => colors.hover,
        };
        commands.entity(entity).insert(ColorTo(color));
    }
}

/// A control bound to a setting: tapping it changes the settings, and
/// it shows whether it is on (an option chosen, a switch on).
#[derive(Component)]
pub struct Control {
    on: Box<dyn Fn(&Settings) -> bool + Send + Sync>,
    tap: Box<dyn Fn(&mut Settings) + Send + Sync>,
    look: Look,
}

enum Look {
    /// One of several options side by side.
    Option { label: Entity },
    /// A switch, its knob sliding to the right when on.
    Switch { knob: Entity },
}

const OPTION: TextStyle = TextStyle::bold(14.0, palette::TEXT);
const OPTION_H: f32 = 34.0;
const SWITCH_W: f32 = 44.0;
const SWITCH_H: f32 = 24.0;
const KNOB: f32 = 18.0;

fn tap_control(tap: On<Tap>, controls: Query<&Control>, mut settings: ResMut<Settings>) {
    if let Ok(control) = controls.get(tap.entity) {
        let mut changed = settings.clone();
        (control.tap)(&mut changed);
        settings.set_if_neq(changed);
    }
}

/// An option among others, chosen when `on` holds; tapping it calls
/// `choose`.
pub fn spawn_option(
    commands: &mut Commands,
    fonts: &Fonts,
    parent: Entity,
    label: &str,
    on: impl Fn(&Settings) -> bool + Send + Sync + 'static,
    choose: impl Fn(&mut Settings) + Send + Sync + 'static,
) -> Entity {
    let option = commands
        .spawn((
            Button,
            Tappable,
            Node {
                height: px(OPTION_H),
                padding: UiRect::horizontal(px(12)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(palette::RAISED),
            ChildOf(parent),
        ))
        .id();
    let label = commands
        .spawn((fonts.text(label, OPTION), ChildOf(option)))
        .id();
    commands.entity(option).insert(Control {
        on: Box::new(on),
        tap: Box::new(choose),
        look: Look::Option { label },
    });
    option
}

/// A switch for a setting `get` reads and `set` writes.
pub fn spawn_switch(
    commands: &mut Commands,
    parent: Entity,
    get: impl Fn(&Settings) -> bool + Send + Sync + Clone + 'static,
    set: impl Fn(&mut Settings, bool) + Send + Sync + 'static,
) -> Entity {
    let switch = commands
        .spawn((
            Button,
            Tappable,
            Node {
                width: px(SWITCH_W),
                height: px(SWITCH_H),
                flex_shrink: 0.0,
                padding: UiRect::all(px((SWITCH_H - KNOB) / 2.0)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(palette::RAISED),
            ChildOf(parent),
        ))
        .id();
    let knob = commands
        .spawn((
            Node {
                width: px(KNOB),
                height: px(KNOB),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(palette::TEXT),
            ChildOf(switch),
        ))
        .id();
    let read = get.clone();
    commands.entity(switch).insert(Control {
        on: Box::new(get),
        tap: Box::new(move |settings| set(settings, !read(settings))),
        look: Look::Switch { knob },
    });
    switch
}

/// Shows every control as the settings stand.
fn show_controls(
    mut commands: Commands,
    settings: Res<Settings>,
    controls: Query<(Entity, &Control)>,
) {
    for (entity, control) in &controls {
        let on = (control.on)(&settings);
        match control.look {
            Look::Option { label } => {
                let (fill, edge, text) = if on {
                    (palette::GOLD, palette::GOLD, palette::BACKGROUND)
                } else {
                    (Color::NONE, palette::RAISED, palette::TEXT)
                };
                commands
                    .entity(entity)
                    .insert((BackgroundColor(fill), BorderColor::all(edge)));
                commands.entity(label).insert(TextColor(text));
            }
            Look::Switch { knob } => {
                let track = if on { palette::GOLD } else { palette::RAISED };
                let justify = if on {
                    JustifyContent::FlexEnd
                } else {
                    JustifyContent::FlexStart
                };
                commands.entity(entity).insert(BackgroundColor(track));
                commands
                    .entity(entity)
                    .entry::<Node>()
                    .and_modify(move |mut node| node.justify_content = justify);
                commands.entity(knob).insert(BackgroundColor(palette::TEXT));
            }
        }
    }
}
