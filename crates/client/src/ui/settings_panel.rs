//! The settings panel, over everything else: who to play against, the
//! board, the look and the animations. Opened from the header; Esc, the
//! Done button or a tap beside the panel closes it. Changes apply at
//! once, except the board size, which applies to the next deal.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use hibedi_bots::BotKind;
use hibedi_core::Side;

use super::Overlay;
use super::tap::{Tap, Tappable};
use super::theme::{Fonts, TextStyle, palette, side_color};
use super::widgets::{self, spawn_option, spawn_switch};
use crate::play::Match;
use crate::settings::{BoardSize, Motion, Opponent, Settings, ThemeKind};

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Overlay::Settings), spawn)
        .add_systems(
            Update,
            (
                close_on_escape,
                scroll,
                show_rows.run_if(
                    resource_changed::<Settings>.or_eager(any_match_filter::<Added<ShownIf>>),
                ),
            )
                .run_if(in_state(Overlay::Settings)),
        );
}

const PAD: f32 = 20.0;
const CARD_W: f32 = 520.0;
const TITLE: TextStyle = TextStyle::bold(20.0, palette::TEXT);
const SECTION: TextStyle = TextStyle::bold(12.0, palette::MUTED);
const LABEL: TextStyle = TextStyle::regular(15.0, palette::TEXT);
const NOTE: TextStyle = TextStyle::regular(13.0, palette::MUTED);
/// Room for the labels before options.
const LABEL_W: f32 = 110.0;
const DONE: TextStyle = TextStyle::bold(15.0, palette::BACKGROUND);
/// Pixels a mouse wheel's line scrolls.
const LINE: f32 = 40.0;

/// The panel's card, which scrolls when it does not fit.
#[derive(Component)]
struct Card;

/// A row shown only while its condition holds.
#[derive(Component)]
struct ShownIf(Box<dyn Fn(&Settings) -> bool + Send + Sync>);

fn spawn(mut commands: Commands, fonts: Res<Fonts>, play: Option<Res<Match>>) {
    // Without a match yet, the game waits for an online one.
    let online = play.is_none_or(|play| play.opponent_online().is_some());

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

    if !online {
        section(&mut commands, &fonts, card, "OPPONENT");
        let (_, options) = labeled(&mut commands, &fonts, card, "Play against");
        spawn_option(
            &mut commands,
            &fonts,
            options,
            "Someone here",
            |settings| settings.opponent == Opponent::Human,
            |settings| settings.opponent = Opponent::Human,
        );
        for bot in BotKind::ALL {
            spawn_option(
                &mut commands,
                &fonts,
                options,
                &format!("{} bot", bot.name()),
                move |settings| settings.opponent == Opponent::Bot(bot),
                move |settings| settings.opponent = Opponent::Bot(bot),
            );
        }
        let is_bot = |settings: &Settings| matches!(settings.opponent, Opponent::Bot(_));
        let (sides_row, sides) = labeled(&mut commands, &fonts, card, "You play");
        for side in Side::ALL {
            let option = spawn_option(
                &mut commands,
                &fonts,
                sides,
                &side.to_string(),
                move |settings| settings.side == side,
                move |settings| settings.side = side,
            );
            commands.spawn((swatch(side_color(side)), ChildOf(option)));
        }
        commands.entity(sides_row).insert(ShownIf(Box::new(is_bot)));
        commands.spawn((
            BotNote,
            ShownIf(Box::new(is_bot)),
            fonts.text("", NOTE),
            Node::default(),
            ChildOf(card),
        ));
    }

    section(&mut commands, &fonts, card, "BOARD");
    let (_, sizes) = labeled(&mut commands, &fonts, card, "Size");
    for size in BoardSize::ALL {
        spawn_option(
            &mut commands,
            &fonts,
            sizes,
            &format!("{}×{}", size.width, size.height),
            move |settings| settings.board == size,
            move |settings| settings.board = size,
        );
    }
    commands.spawn((
        fonts.text("A new size applies from the next game on.", NOTE),
        ChildOf(card),
    ));
    let (_, themes) = labeled(&mut commands, &fonts, card, "Look");
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
    let switches: [(&str, fn(&mut Motion) -> &mut bool); 6] = [
        ("Tokens hop", |motion| &mut motion.hops),
        ("Cells flip over", |motion| &mut motion.flips),
        ("Things pop in and out", |motion| &mut motion.pops),
        ("Hints pulse", |motion| &mut motion.pulses),
        ("Winning cells cheer", |motion| &mut motion.cheers),
        ("Colors fade", |motion| &mut motion.fades),
    ];
    for (label, field) in switches {
        let line = row(&mut commands, card);
        commands.spawn((
            fonts.text(label, LABEL),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(line),
        ));
        spawn_switch(
            &mut commands,
            line,
            move |settings| *field(&mut settings.motion.clone()),
            move |settings, on| *field(&mut settings.motion) = on,
        );
    }
}

/// Explains the bot chosen.
#[derive(Component)]
struct BotNote;

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

/// A row with a label, and room for options after it (wrapping onto
/// further lines on narrow screens); returns the row and the options'
/// container.
fn labeled(
    commands: &mut Commands,
    fonts: &Fonts,
    parent: Entity,
    label: &str,
) -> (Entity, Entity) {
    let line = commands
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                column_gap: px(12),
                row_gap: px(8),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    commands.spawn((
        fonts.text(label, LABEL),
        // On one line, so it centers on the options.
        TextLayout::no_wrap(),
        Node {
            width: px(LABEL_W),
            ..default()
        },
        ChildOf(line),
    ));
    let options = commands
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                ..default()
            },
            ChildOf(line),
        ))
        .id();
    (line, options)
}

fn swatch(color: Color) -> impl Bundle {
    (
        Node {
            width: px(10),
            height: px(10),
            margin: UiRect::left(px(8)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(color),
    )
}

/// Shows the rows that apply to the settings, and explains the bot.
fn show_rows(
    settings: Res<Settings>,
    mut rows: Query<(&ShownIf, &mut Node)>,
    mut notes: Query<&mut Text, With<BotNote>>,
) {
    for (ShownIf(condition), mut node) in &mut rows {
        let display = if condition(&settings) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    if let Opponent::Bot(bot) = settings.opponent {
        for mut note in &mut notes {
            note.0 = bot.description().to_owned();
        }
    }
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
