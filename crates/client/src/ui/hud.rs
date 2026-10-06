//! Everything around the board: the presence pill in the header, the
//! sidebar (score, whose turn it is and what to do, new game), and the
//! notice shown while there is no game yet.

use bevy::prelude::*;
use hibedi_core::{Game, GameResult, Phase, Side, WinReason};

use super::Screen;
use super::anim::{Animate, ColorTo, ScaleTo};
use super::layout::{Content, Header, Sidebar, WIDE_CARD_W};
use super::tap::{Tap, Tappable};
use super::theme::{Fonts, TextStyle, palette, side_color};
use crate::play::Match;

pub fn plugin(app: &mut App) {
    app.init_resource::<Notice>().add_systems(
        Update,
        (
            show_notice.run_if(resource_changed::<Notice>),
            (show_presence, show_score, show_turn, show_new_game)
                .run_if(in_state(Screen::Playing).and_then(resource_changed::<Match>)),
            hover_buttons,
        )
            .before(Animate),
    );
}

/// What the screen says while there is no game to show yet.
#[derive(Resource, Clone, PartialEq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

impl Notice {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
        }
    }
}

impl Default for Notice {
    /// Online, the second seat waits briefly for the first seat's board.
    fn default() -> Self {
        Self::new("Starting…", "Waiting for the board from the other player.")
    }
}

const CARD_PAD: f32 = 18.0;
const CARD_RADIUS: f32 = 12.0;
const CARD_TITLE: TextStyle = TextStyle::bold(18.0, palette::TEXT);
const CARD_BODY: TextStyle = TextStyle::regular(15.0, palette::MUTED);
/// Diameter of the color swatches before titles and names.
const SWATCH: f32 = 12.0;

const PILL_TEXT: TextStyle = TextStyle::bold(14.0, palette::TEXT);
const SEAT_DOT: f32 = 10.0;

const NAME: TextStyle = TextStyle::bold(16.0, palette::TEXT);
const YOU: TextStyle = TextStyle::bold(11.0, palette::MUTED);
const COUNT: TextStyle = TextStyle::bold(24.0, palette::TEXT);

const BUTTON_H: f32 = 44.0;
const BUTTON: TextStyle = TextStyle::bold(15.0, palette::TEXT);

// --- spawning ---

/// The pill in the header and the notice card.
pub fn spawn(
    mut commands: Commands,
    fonts: Res<Fonts>,
    header: Single<Entity, With<Header>>,
    content: Single<Entity, With<Content>>,
) {
    spawn_pill(&mut commands, &fonts, *header);
    let notice = spawn_text_card(&mut commands, &fonts, *content);
    commands.entity(notice).insert((
        NoticeCard,
        DespawnOnExit(Screen::Waiting),
        Node {
            width: percent(100),
            max_width: px(WIDE_CARD_W),
            ..card_node()
        },
    ));
}

/// The sidebar: score, turn card and the new-game button.
pub fn spawn_sidebar(
    mut commands: Commands,
    fonts: Res<Fonts>,
    content: Single<Entity, With<Content>>,
) {
    let sidebar = commands
        .spawn((Sidebar, Sidebar::node(), ChildOf(*content)))
        .id();
    spawn_score(&mut commands, &fonts, sidebar);
    let turn = spawn_text_card(&mut commands, &fonts, sidebar);
    commands.entity(turn).insert(TurnCard);
    spawn_new_game_button(&mut commands, &fonts, sidebar);
}

fn card_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(CARD_PAD)),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(CARD_RADIUS)),
        ..default()
    }
}

/// A panel laid out by `node`.
fn card(node: Node) -> impl Bundle {
    (
        node,
        BackgroundColor(palette::PANEL),
        BorderColor::all(palette::RAISED),
    )
}

fn swatch(size: f32) -> Node {
    Node {
        width: px(size),
        height: px(size),
        flex_shrink: 0.0,
        border_radius: BorderRadius::MAX,
        ..default()
    }
}

// --- text cards ---

#[derive(Component)]
struct NoticeCard;

#[derive(Component)]
struct TurnCard;

/// A card with an optional color swatch, a title line and a body.
#[derive(Component)]
struct TextCardParts {
    swatch: Entity,
    title: Entity,
    body: Entity,
}

fn spawn_text_card(commands: &mut Commands, fonts: &Fonts, parent: Entity) -> Entity {
    let card = commands.spawn((card(card_node()), ChildOf(parent))).id();
    let title_row = commands
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(10),
                margin: UiRect::bottom(px(8)),
                ..default()
            },
            ChildOf(card),
        ))
        .id();
    let swatch = commands
        .spawn((swatch(SWATCH), UiTransform::IDENTITY, ChildOf(title_row)))
        .id();
    let title = commands
        .spawn((fonts.text("", CARD_TITLE), ChildOf(title_row)))
        .id();
    let body = commands
        .spawn((fonts.text("", CARD_BODY), ChildOf(card)))
        .id();
    commands.entity(card).insert(TextCardParts {
        swatch,
        title,
        body,
    });
    card
}

/// Shows a title and body; with an accent color, a swatch precedes the
/// title. A new title draws the eye with a bump of the swatch.
fn show_text_card(
    commands: &mut Commands,
    parts: &TextCardParts,
    texts: &Query<&Text>,
    accent: Option<Color>,
    title: &str,
    body: &str,
) {
    let new_title = texts.get(parts.title).is_ok_and(|shown| shown.0 != title);
    let mut swatch = commands.entity(parts.swatch);
    match accent {
        Some(color) => {
            swatch.insert((swatch_node(true), BackgroundColor(color)));
            if new_title {
                swatch.insert(ScaleTo::bump());
            }
        }
        None => {
            swatch.insert(swatch_node(false));
        }
    }
    commands.entity(parts.title).insert(Text::new(title));
    commands.entity(parts.body).insert(Text::new(body));
}

fn swatch_node(shown: bool) -> Node {
    Node {
        display: if shown { Display::Flex } else { Display::None },
        ..swatch(SWATCH)
    }
}

fn show_notice(
    mut commands: Commands,
    notice: Res<Notice>,
    card: Query<&TextCardParts, With<NoticeCard>>,
    texts: Query<&Text>,
) {
    for parts in &card {
        show_text_card(
            &mut commands,
            parts,
            &texts,
            None,
            &notice.title,
            &notice.body,
        );
    }
}

fn show_turn(
    mut commands: Commands,
    play: Res<Match>,
    card: Single<&TextCardParts, With<TurnCard>>,
    texts: Query<&Text>,
) {
    let Some(game) = play.game() else {
        return;
    };
    let turn = TurnText::of(&play, game);
    show_text_card(
        &mut commands,
        &card,
        &texts,
        turn.side.map(side_color),
        &turn.title,
        &turn.body,
    );
}

struct TurnText {
    side: Option<Side>,
    title: String,
    body: String,
}

impl TurnText {
    fn of(play: &Match, game: &Game) -> Self {
        let text = |side: Option<Side>, title: String, body: String| Self { side, title, body };

        if let Some(result) = game.result() {
            return match result {
                GameResult::Winner { side, reason } => {
                    let color = side.to_string().to_lowercase();
                    text(
                        Some(side),
                        format!("{side} wins"),
                        match reason {
                            WinReason::Monochrome => format!("Every colored cell is {color}."),
                            WinReason::Majority => {
                                format!("No moves are left, and {color} holds more colored cells.")
                            }
                        },
                    )
                }
                GameResult::Draw => text(
                    None,
                    "Draw".to_owned(),
                    "No moves are left, and both colors hold equally many cells.".to_owned(),
                ),
            };
        }

        let current = game.current_player();
        let to_move = format!("{current} to move");
        match play.opponent_online() {
            None => text(Some(current), to_move, instruction(game).to_owned()),
            Some(false) => text(
                Some(current),
                to_move,
                "Your opponent is offline. The game continues when they are back.".to_owned(),
            ),
            Some(true) if play.my_side() == Some(current) => text(
                Some(current),
                "Your move".to_owned(),
                instruction(game).to_owned(),
            ),
            Some(true) => text(
                Some(current),
                to_move,
                "Waiting for your opponent.".to_owned(),
            ),
        }
    }
}

/// What the player to move does next.
fn instruction(game: &Game) -> &'static str {
    match game.phase() {
        Phase::PlaceInitialToken => "Place a token on a cyan-marked cell.",
        Phase::ChooseJump if !game.selectable_activated_tokens().is_empty() => {
            "Jump to a gold-marked cell, or switch to another glowing token."
        }
        Phase::ChooseJump => "Jump to a gold-marked cell. Every available jump is compulsory.",
        Phase::ChooseActivatedToken => "Choose a glowing token to jump with.",
        Phase::GameOver => "",
    }
}

// --- presence ---

/// The players at a glance (online only): one dot per side, ours ringed
/// white, the opponent's filled while online and hollow while away.
#[derive(Component)]
struct PresencePill;

#[derive(Component)]
struct SeatDot(Side);

#[derive(Component)]
struct PresenceText;

fn spawn_pill(commands: &mut Commands, fonts: &Fonts, header: Entity) {
    let pill = commands
        .spawn((
            PresencePill,
            Node {
                display: Display::None,
                height: px(32),
                padding: UiRect::horizontal(px(12)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::MAX,
                align_items: AlignItems::Center,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(palette::PANEL),
            BorderColor::all(palette::RAISED),
            ChildOf(header),
        ))
        .id();
    for side in Side::ALL {
        commands.spawn((
            SeatDot(side),
            Node {
                border: UiRect::all(px(2)),
                ..swatch(SEAT_DOT)
            },
            BorderColor::all(side_color(side)),
            ChildOf(pill),
        ));
    }
    commands.spawn((
        PresenceText,
        fonts.text("", PILL_TEXT),
        Node {
            margin: UiRect::left(px(4)),
            ..default()
        },
        ChildOf(pill),
    ));
}

fn show_presence(
    mut commands: Commands,
    play: Res<Match>,
    mut pill: Single<&mut Node, With<PresencePill>>,
    dots: Query<(Entity, &SeatDot)>,
    text: Single<Entity, With<PresenceText>>,
) {
    let (Some(me), Some(online)) = (play.my_side(), play.opponent_online()) else {
        return;
    };
    pill.display = Display::Flex;
    for (entity, &SeatDot(side)) in &dots {
        let fill = if side == me || online {
            side_color(side)
        } else {
            Color::NONE
        };
        let ring = if side == me {
            Outline::new(px(1.5), px(1.5), palette::OWN_EDGE)
        } else {
            Outline::new(px(0), px(0), Color::NONE)
        };
        commands
            .entity(entity)
            .insert((BackgroundColor(fill), ring));
    }
    let label = if online {
        "Opponent online"
    } else {
        "Opponent offline"
    };
    commands.entity(*text).insert(Text::new(label));
}

// --- score ---

/// One side's colored-cell count; our own side (online) is highlighted.
#[derive(Component)]
struct ScoreTile(Side);

#[derive(Component)]
struct ScoreCount(Side);

#[derive(Component)]
struct YouTag(Side);

fn spawn_score(commands: &mut Commands, fonts: &Fonts, sidebar: Entity) {
    let card = commands
        .spawn((
            card(Node {
                flex_direction: FlexDirection::Row,
                padding: UiRect::all(px(8)),
                column_gap: px(8),
                ..card_node()
            }),
            ChildOf(sidebar),
        ))
        .id();
    for side in Side::ALL {
        let tile = commands
            .spawn((
                ScoreTile(side),
                Node {
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    height: px(46),
                    padding: UiRect::horizontal(px(12)),
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    border_radius: BorderRadius::all(px(CARD_RADIUS - 4.0)),
                    ..default()
                },
                ChildOf(card),
            ))
            .id();
        commands.spawn((
            swatch(SWATCH),
            BackgroundColor(side_color(side)),
            ChildOf(tile),
        ));
        commands.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..default()
            },
            children![
                fonts.text(side.to_string(), NAME),
                (
                    YouTag(side),
                    fonts.text("YOU", YOU),
                    Node {
                        display: Display::None,
                        ..default()
                    },
                ),
            ],
            ChildOf(tile),
        ));
        commands.spawn((
            ScoreCount(side),
            fonts.text("", COUNT),
            UiTransform::IDENTITY,
            ChildOf(tile),
        ));
    }
}

fn show_score(
    mut commands: Commands,
    play: Res<Match>,
    tiles: Query<(Entity, &ScoreTile)>,
    counts: Query<(Entity, &ScoreCount, &Text)>,
    tags: Query<(Entity, &YouTag)>,
) {
    let Some(game) = play.game() else {
        return;
    };
    let mine = |side: Side| play.my_side() == Some(side);
    for (entity, &ScoreTile(side)) in &tiles {
        let fill = if mine(side) {
            palette::RAISED
        } else {
            Color::NONE
        };
        commands.entity(entity).insert(BackgroundColor(fill));
    }
    for (entity, &YouTag(side)) in &tags {
        let display = if mine(side) {
            Display::Flex
        } else {
            Display::None
        };
        commands.entity(entity).insert(Node {
            display,
            ..default()
        });
    }
    for (entity, &ScoreCount(side), shown) in &counts {
        let count = game.board().colored_count(side).to_string();
        if shown.0 != count {
            let mut text = commands.entity(entity);
            text.insert(Text::new(count));
            // The very first count appears quietly.
            if !shown.0.is_empty() {
                text.insert(ScaleTo::bump());
            }
        }
    }
}

// --- new game ---

#[derive(Component)]
struct NewGameButton;

#[derive(Component)]
struct NewGameLabel;

/// A button's background at rest and under the pointer.
#[derive(Component, PartialEq)]
struct ButtonColors {
    rest: Color,
    hover: Color,
}

fn spawn_new_game_button(commands: &mut Commands, fonts: &Fonts, sidebar: Entity) {
    commands
        .spawn((
            NewGameButton,
            Button,
            Tappable,
            Node {
                // The slot stays when the button is hidden, so nothing
                // moves when it appears.
                height: px(BUTTON_H),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(palette::RAISED),
            Visibility::Hidden,
            UiTransform::IDENTITY,
            ChildOf(sidebar),
            children![(NewGameLabel, fonts.text("New game", BUTTON))],
        ))
        .observe(|_: On<Tap>, mut play: ResMut<Match>| play.start_new_game());
}

/// Hotseat may restart any time; once a game is over, the button turns
/// gold (with dark text: light text lacks contrast against it).
fn show_new_game(
    mut commands: Commands,
    play: Res<Match>,
    button: Single<(Entity, &Visibility), With<NewGameButton>>,
    label: Single<Entity, With<NewGameLabel>>,
) {
    let (entity, visibility) = *button;
    let shown = play.can_start_new_game();
    let primary = play.game().is_some_and(|game| game.result().is_some());
    let (rest, hover, text) = if primary {
        (
            palette::GOLD,
            palette::GOLD.lighter(0.08),
            palette::BACKGROUND,
        )
    } else {
        (
            palette::RAISED,
            palette::RAISED.lighter(0.08),
            palette::TEXT,
        )
    };

    let mut button = commands.entity(entity);
    button.insert(ButtonColors { rest, hover });
    if shown && *visibility == Visibility::Hidden {
        button.insert((Visibility::Inherited, ScaleTo::appear(0.0)));
    } else if !shown {
        button.insert(Visibility::Hidden);
    }
    commands.entity(*label).insert(TextColor(text));
}

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
