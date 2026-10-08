//! The settings panel, over everything else: the board's look and the
//! animations, which change at once. Opened from the header; Esc, the
//! Done button or a tap beside the panel closes it. (What a game is
//! dealt with is chosen beside the board, for the next game.)

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use super::Overlay;
use super::tap::{Tap, Tappable};
use super::theme::{Fonts, TextStyle, palette};
use super::widgets::{self, spawn_choice, spawn_option, spawn_switch};
use crate::settings::{Animation, ThemeKind};

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Overlay::Settings), spawn)
        .add_systems(
            Update,
            (close_on_escape, scroll).run_if(in_state(Overlay::Settings)),
        );
}

const PAD: f32 = 20.0;
const CARD_W: f32 = 520.0;
const TITLE: TextStyle = TextStyle::bold(20.0, palette::TEXT);
const SECTION: TextStyle = TextStyle::bold(12.0, palette::MUTED);
const LABEL: TextStyle = TextStyle::regular(15.0, palette::TEXT);
const DONE: TextStyle = TextStyle::bold(15.0, palette::BACKGROUND);
/// Pixels a mouse wheel's line scrolls.
const LINE: f32 = 40.0;

/// The panel's card, which scrolls when it does not fit.
#[derive(Component)]
struct Card;

fn spawn(mut commands: Commands, fonts: Res<Fonts>) {
    let backdrop = commands
        .spawn((
            DespawnOnExit(Overlay::Settings),
            Tappable,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(16)),
                ..default()
            },
            BackgroundColor(Color::BLACK.with_alpha(0.55)),
            GlobalZIndex(10),
        ))
        .observe(close)
        .id();
    let card = commands
        .spawn((
            Card,
            // Taps on the card stay on it.
            Tappable,
            Node {
                width: percent(100),
                max_width: px(CARD_W),
                max_height: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(PAD)),
                row_gap: px(14),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(12)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            BackgroundColor(palette::PANEL),
            BorderColor::all(palette::RAISED),
            ChildOf(backdrop),
        ))
        .id();

    let title = row(&mut commands, card);
    commands.spawn((
        fonts.text("Settings", TITLE),
        Node {
            flex_grow: 1.0,
            ..default()
        },
        ChildOf(title),
    ));
    commands
        .spawn((
            widgets::button(
                &fonts,
                "Done",
                DONE,
                palette::GOLD,
                Node {
                    height: px(36),
                    padding: UiRect::horizontal(px(16)),
                    ..default()
                },
            ),
            ChildOf(title),
        ))
        .observe(close);

    let (_, themes) = spawn_choice(&mut commands, &fonts, card, "LOOK");
    for theme in ThemeKind::ALL {
        spawn_option(
            &mut commands,
            &fonts,
            themes,
            theme.name(),
            move |settings| settings.theme == theme,
            move |settings| settings.theme = theme,
        );
    }

    section(&mut commands, &fonts, card, "ANIMATIONS");
    for animation in Animation::ALL {
        let line = row(&mut commands, card);
        commands.spawn((
            fonts.text(animation.label(), LABEL),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(line),
        ));
        spawn_switch(
            &mut commands,
            line,
            move |settings| settings.motion.plays(animation),
            move |settings, on| settings.motion.set(animation, on),
        );
    }
}

fn row(commands: &mut Commands, parent: Entity) -> Entity {
    commands
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(12),
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn section(commands: &mut Commands, fonts: &Fonts, parent: Entity, title: &str) {
    commands.spawn((
        fonts.text(title, SECTION),
        Node {
            margin: UiRect::top(px(6)),
            ..default()
        },
        ChildOf(parent),
    ));
}

fn close(_: On<Tap>, mut overlay: ResMut<NextState<Overlay>>) {
    overlay.set(Overlay::None);
}

fn close_on_escape(keys: Res<ButtonInput<KeyCode>>, mut overlay: ResMut<NextState<Overlay>>) {
    if keys.just_pressed(KeyCode::Escape) {
        overlay.set(Overlay::None);
    }
}

/// Scrolls the card with the mouse wheel, or a finger dragged across.
fn scroll(
    mut wheel: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    card: Single<(&mut ScrollPosition, &ComputedNode), With<Card>>,
) {
    let (mut position, node) = card.into_inner();
    let mut by = 0.0;
    for event in wheel.read() {
        by -= match event.unit {
            MouseScrollUnit::Line => event.y * LINE,
            MouseScrollUnit::Pixel => event.y,
        };
    }
    for touch in touches.iter() {
        by -= touch.delta().y;
    }
    if by == 0.0 {
        return;
    }
    let scale = node.inverse_scale_factor();
    let overflow = ((node.content_size().y - node.size().y) * scale).max(0.0);
    position.y = (position.y + by).clamp(0.0, overflow);
}
