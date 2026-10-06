//! The screen's skeleton, and how it adapts to the window.
//!
//! A header band runs along the top (title left, connection status
//! right). Below it, the content: a notice while there is no game, then
//! the board and the sidebar as one group, either side by side or
//! stacked (sidebar under the board), whichever gives the board larger
//! cells.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::Screen;
use super::board::{self, BoardFrame};
use super::theme::{Fonts, TextStyle, palette};
use crate::play::Match;

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (adapt_to_window, fit_board.run_if(in_state(Screen::Playing))),
    );
}

/// Screen margin; [`COMPACT_PAD`] on small screens.
const PAD: f32 = 24.0;
const COMPACT_PAD: f32 = 16.0;
/// Screens whose short side is below this are compact.
const COMPACT_BELOW: f32 = 600.0;
const HEADER_H: f32 = 44.0;
/// Gap between the board and the sidebar, and between cards.
pub const GAP: f32 = 16.0;
/// Sidebar width beside the board.
const SIDEBAR_W: f32 = 280.0;
/// Widest a stacked sidebar or a notice card grows.
pub const WIDE_CARD_W: f32 = 460.0;
/// Below this cell size the board is not worth laying out larger.
const MIN_CELL: f32 = 12.0;

const TITLE: TextStyle = TextStyle::bold(26.0, palette::TEXT);
const COMPACT_TITLE: f32 = 20.0;
const SUBTITLE: TextStyle = TextStyle::regular(14.0, palette::MUTED);

#[derive(Component)]
struct Root;

#[derive(Component)]
struct Title;

#[derive(Component)]
struct Subtitle;

/// The header band; the title is on its left, status goes on its right.
#[derive(Component)]
pub struct Header;

/// Everything below the header.
#[derive(Component)]
pub struct Content;

/// The sidebar beside or below the board.
#[derive(Component)]
pub struct Sidebar;

impl Sidebar {
    pub fn node() -> Node {
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(GAP),
            flex_shrink: 0.0,
            width: px(SIDEBAR_W),
            max_width: px(WIDE_CARD_W),
            ..default()
        }
    }
}

pub fn spawn(mut commands: Commands, fonts: Res<Fonts>) {
    commands.spawn((
        Root,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(PAD)),
            row_gap: px(PAD),
            ..default()
        },
        children![
            (
                Header,
                Node {
                    height: px(HEADER_H),
                    flex_shrink: 0.0,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    column_gap: px(GAP),
                    ..default()
                },
                children![(
                    Node {
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    children![
                        (Title, fonts.text("HIBE DI HOPEDI", TITLE)),
                        (
                            Subtitle,
                            fonts.text("A forced-hop territory prototype", SUBTITLE)
                        ),
                    ],
                )],
            ),
            (
                Content,
                Node {
                    flex_grow: 1.0,
                    min_height: px(0),
                    justify_content: JustifyContent::Center,
                    column_gap: px(GAP),
                    row_gap: px(GAP),
                    ..default()
                },
            ),
        ],
    ));
}

/// Tightens the margins and the header on small screens.
fn adapt_to_window(
    window: Single<&Window, With<PrimaryWindow>>,
    mut root: Single<&mut Node, With<Root>>,
    mut subtitle: Single<&mut Node, (With<Subtitle>, Without<Root>)>,
    mut title: Single<&mut TextFont, With<Title>>,
) {
    let compact = window.width().min(window.height()) < COMPACT_BELOW;
    let pad = px(if compact { COMPACT_PAD } else { PAD });
    let title_size = if compact { COMPACT_TITLE } else { TITLE.size };

    restyle(&mut root, |root| {
        root.padding = UiRect::all(pad);
        root.row_gap = pad;
    });
    restyle(&mut subtitle, |subtitle| {
        subtitle.display = if compact {
            Display::None
        } else {
            Display::Flex
        };
    });
    if title.font_size != FontSize::Px(title_size) {
        title.font_size = FontSize::Px(title_size);
    }
}

/// Sizes the board to whole-pixel cells as large as fit, beside or
/// above the sidebar, whichever allows larger ones.
fn fit_board(
    play: Res<Match>,
    content: Single<(&ComputedNode, &mut Node), With<Content>>,
    sidebar: Single<(&ComputedNode, &mut Node), (With<Sidebar>, Without<Content>)>,
    mut frame: Single<&mut Node, (With<BoardFrame>, Without<Content>, Without<Sidebar>)>,
) {
    let Some(game) = play.game() else {
        return;
    };
    let columns = game.board().storage_width() as f32;
    let rows = game.board().storage_height() as f32;
    let (content_size, mut content) = content.into_inner();
    let (sidebar_size, mut sidebar) = sidebar.into_inner();
    let available = content_size.size() * content_size.inverse_scale_factor();
    let sidebar_h = sidebar_size.size().y * sidebar_size.inverse_scale_factor();

    let frame_w = 2.0 * board::FRAME;
    let cell_for = |width: f32, height: f32| {
        ((width - frame_w) / columns)
            .min((height - frame_w) / rows)
            .floor()
            .max(MIN_CELL)
    };
    let beside = cell_for(available.x - GAP - SIDEBAR_W, available.y);
    let stacked = cell_for(available.x, available.y - GAP - sidebar_h);

    let (direction, align, cell, sidebar_w) = if beside >= stacked {
        (
            FlexDirection::Row,
            AlignItems::FlexStart,
            beside,
            px(SIDEBAR_W),
        )
    } else {
        (
            FlexDirection::Column,
            AlignItems::Center,
            stacked,
            percent(100),
        )
    };
    restyle(&mut content, |content| {
        content.flex_direction = direction;
        content.align_items = align;
    });
    restyle(&mut sidebar, |sidebar| sidebar.width = sidebar_w);
    restyle(&mut frame, |frame| {
        frame.width = px(columns * cell + frame_w);
        frame.height = px(rows * cell + frame_w);
    });
}

/// Changes a node's style only if `change` alters it: every change
/// triggers a new layout.
fn restyle(node: &mut Mut<Node>, change: impl FnOnce(&mut Node)) {
    let mut next = node.clone();
    change(&mut next);
    node.set_if_neq(next);
}
