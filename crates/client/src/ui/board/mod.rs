//! The board: a grid of cells with tokens on top, following the match.
//!
//! The entities only ever follow a [`BoardView`] of the current game.
//! Whenever the match changes, the new view is compared with what is
//! shown and the differences animate: tokens hop to where they moved,
//! crossed cells flip as the hopping token passes over them, and a new
//! deal flips the board over in a wave. Clicks on cells go to the match.

mod view;

use bevy::prelude::*;
use hibedi_core::{Cell, Coord, GameResult};

use super::Screen;
use super::anim::{Animate, Bounce, ColorTo, Flip, Hop, Pulse, ScaleTo};
use super::layout::Content;
use super::tap::{Tap, Tappable};
use super::theme::{palette, side_color};
use crate::play::Match;
use view::{BoardView, CellView, Halo, Hint, TokenView};

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        follow_match
            .run_if(in_state(Screen::Playing).and_then(resource_changed::<Match>))
            .before(Animate),
    );
}

/// Width of the frame around the cells, in pixels.
pub const FRAME: f32 = 6.0;
/// Seconds between neighboring diagonals of a wave across the board.
const WAVE_STEP: f32 = 0.035;
/// A hop takes this long, plus [`HOP_PER_CELL`] for each cell it spans.
const HOP_BASE: f32 = 0.16;
const HOP_PER_CELL: f32 = 0.07;

/// The board's frame; the layout sizes it.
#[derive(Component)]
pub struct BoardFrame;

/// The grid of cells inside the frame; tokens are positioned on it in
/// percent of its size.
#[derive(Component)]
struct BoardGrid {
    columns: usize,
    rows: usize,
}

impl BoardGrid {
    /// The `left`/`top` of a cell, in percent of the grid.
    fn position(&self, coord: Coord) -> Vec2 {
        vec2(
            coord.x as f32 * 100.0 / self.columns as f32,
            coord.y as f32 * 100.0 / self.rows as f32,
        )
    }
}

#[derive(Component)]
struct BoardCell {
    coord: Coord,
    /// The color shown, or being flipped to.
    shown: Cell,
}

/// A cell's decorations, bottom to top.
#[derive(Component)]
struct CellParts {
    /// Dims crossed cells.
    overlay: Entity,
    /// Marks the viewer's own cells.
    frame: Entity,
    /// The hint marker: a ring around a dot.
    ring: Entity,
    dot: Entity,
}

#[derive(Component)]
struct BoardToken {
    view: TokenView,
}

#[derive(Component)]
struct TokenParts {
    halo: Entity,
    disc: Entity,
}

/// Builds the board for the match's game, growing in with a wave.
pub fn spawn(mut commands: Commands, play: Res<Match>, content: Single<Entity, With<Content>>) {
    let game = play.game().expect("a game is being played");
    let view = BoardView::of(game, play.shows_hints(), play.my_side());
    let grid = BoardGrid {
        columns: game.board().storage_width(),
        rows: game.board().storage_height(),
    };

    let frame = commands
        .spawn((
            BoardFrame,
            Node {
                padding: UiRect::all(px(FRAME)),
                border_radius: BorderRadius::all(px(14)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(palette::PANEL),
            ChildOf(*content),
        ))
        .id();
    let grid_entity = commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                display: Display::Grid,
                grid_template_columns: RepeatedGridTrack::flex(grid.columns as u16, 1.0),
                grid_template_rows: RepeatedGridTrack::flex(grid.rows as u16, 1.0),
                ..default()
            },
            ChildOf(frame),
        ))
        .id();

    for cell in &view.cells {
        spawn_cell(&mut commands, grid_entity, cell);
    }
    for token in &view.tokens {
        spawn_token(
            &mut commands,
            grid_entity,
            &grid,
            token,
            wave_delay(token.position),
        );
    }
    commands.entity(grid_entity).insert(grid);
}

fn spawn_cell(commands: &mut Commands, grid: Entity, view: &CellView) {
    let cell = commands
        .spawn((
            BoardCell {
                coord: view.coord,
                shown: view.cell,
            },
            Node {
                margin: UiRect::all(px(1)),
                border_radius: BorderRadius::all(percent(8)),
                ..default()
            },
            BackgroundColor(cell_color(view.cell)),
            ScaleTo::appear(wave_delay(view.coord)),
            Tappable,
            ChildOf(grid),
        ))
        .observe(tap_cell)
        .id();
    let mut part = |node: Node| commands.spawn((node, ChildOf(cell))).id();
    let overlay = part(Node {
        border_radius: BorderRadius::all(percent(8)),
        ..centered(100.0)
    });
    let frame = part(Node {
        border: UiRect::all(percent(3)),
        border_radius: BorderRadius::all(percent(6)),
        ..centered(86.0)
    });
    let ring = part(Node {
        border: UiRect::all(percent(5)),
        border_radius: BorderRadius::MAX,
        ..centered(56.0)
    });
    let dot = commands
        .spawn((
            Node {
                border_radius: BorderRadius::MAX,
                ..centered(36.0)
            },
            ChildOf(ring),
        ))
        .id();
    commands
        .entity(frame)
        .insert(BorderColor::all(palette::OWN_EDGE));
    commands.entity(ring).insert(Pulse { amount: 0.06 });

    let parts = CellParts {
        overlay,
        frame,
        ring,
        dot,
    };
    show_cell(commands, &parts, view);
    commands.entity(cell).insert(parts);
}

/// Applies a cell's decorations; its color is the caller's business.
fn show_cell(commands: &mut Commands, parts: &CellParts, view: &CellView) {
    let overlay = if view.used {
        palette::USED_CELL_OVERLAY
    } else {
        Color::NONE
    };
    commands.entity(parts.overlay).insert(ColorTo(overlay));
    commands.entity(parts.frame).insert(visible_if(view.own));

    commands
        .entity(parts.ring)
        .insert(visible_if(view.hint.is_some()));
    if let Some(hint) = view.hint {
        let color = match hint {
            Hint::Place => palette::CYAN,
            Hint::Land => palette::GOLD,
        };
        commands.entity(parts.ring).insert(BorderColor::all(color));
        commands.entity(parts.dot).insert(BackgroundColor(color));
    }
}

fn spawn_token(
    commands: &mut Commands,
    grid_entity: Entity,
    grid: &BoardGrid,
    view: &TokenView,
    delay: f32,
) {
    let at = grid.position(view.position);
    let token = commands
        .spawn((
            BoardToken { view: *view },
            Node {
                position_type: PositionType::Absolute,
                left: percent(at.x),
                top: percent(at.y),
                width: percent(100.0 / grid.columns as f32),
                height: percent(100.0 / grid.rows as f32),
                ..default()
            },
            ScaleTo::appear(delay),
            ChildOf(grid_entity),
        ))
        .id();
    let halo = commands
        .spawn((
            Node {
                border: UiRect::all(percent(2.5)),
                border_radius: BorderRadius::MAX,
                ..centered(76.0)
            },
            ChildOf(token),
        ))
        .id();
    let disc = commands
        .spawn((
            BackgroundColor(palette::TOKEN_FILL),
            BoxShadow::new(palette::SHADOW, px(0), percent(6), px(0), percent(8)),
            ChildOf(token),
        ))
        .id();

    let parts = TokenParts { halo, disc };
    show_token(commands, &parts, view);
    commands.entity(token).insert(parts);
}

/// Applies a token's rim and halo.
fn show_token(commands: &mut Commands, parts: &TokenParts, view: &TokenView) {
    // The token jumping wears a thick cyan rim, the others a thin light
    // one (in percent of the token's cell).
    let (rim, rim_color) = if view.jumping {
        (7.0, palette::CYAN)
    } else {
        (3.0, palette::TOKEN_EDGE)
    };
    commands.entity(parts.disc).insert((
        Node {
            border: UiRect::all(percent(rim)),
            border_radius: BorderRadius::MAX,
            ..centered(54.0)
        },
        BorderColor::all(rim_color),
    ));

    let (visibility, fill, ring) = match view.halo {
        Halo::None => (Visibility::Hidden, Color::NONE, Color::NONE),
        Halo::Selectable => (Visibility::Inherited, palette::GOLD, Color::NONE),
        Halo::Pending => (Visibility::Inherited, Color::NONE, palette::MUTED),
    };
    let mut halo = commands.entity(parts.halo);
    halo.insert((visibility, BackgroundColor(fill), BorderColor::all(ring)));
    if view.halo == Halo::Selectable {
        halo.insert(Pulse { amount: 0.07 });
    } else {
        halo.remove::<Pulse>().insert(UiTransform::IDENTITY);
    }
}

/// An absolutely positioned node `size` percent of its parent, centered
/// in it.
fn centered(size: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: percent((100.0 - size) / 2.0),
        top: percent((100.0 - size) / 2.0),
        width: percent(size),
        height: percent(size),
        ..default()
    }
}

/// Brings the board up to date with the match, animating every change.
fn follow_match(
    mut commands: Commands,
    play: Res<Match>,
    grid: Single<(Entity, &BoardGrid)>,
    mut cells: Query<(Entity, &mut BoardCell, &CellParts)>,
    tokens: Query<(Entity, &BoardToken, &TokenParts, &Node)>,
    mut shown_result: Local<Option<GameResult>>,
) {
    let Some(game) = play.game() else {
        return;
    };
    let view = BoardView::of(game, play.shows_hints(), play.my_side());
    let (grid_entity, grid) = *grid;

    // Tokens first: where they hop decides when the cells they cross
    // flip.
    let mut hops = Vec::new();
    for (entity, token, parts, node) in &tokens {
        let Some(next) = view.tokens.iter().find(|next| next.id == token.view.id) else {
            commands
                .entity(entity)
                .remove::<BoardToken>()
                .insert(ScaleTo::disappear());
            continue;
        };
        if next.position != token.view.position {
            let from = token.view.position;
            let span = distance(from, next.position);
            let duration = HOP_BASE + HOP_PER_CELL * span;
            hops.push(Path {
                from,
                span,
                duration,
            });
            commands.entity(entity).remove::<ScaleTo>().insert(Hop::new(
                position_of(node),
                grid.position(next.position),
                duration,
            ));
        }
        commands.entity(entity).insert(BoardToken { view: *next });
        show_token(&mut commands, parts, next);
    }
    for token in &view.tokens {
        if !tokens
            .iter()
            .any(|(_, shown, ..)| shown.view.id == token.id)
        {
            spawn_token(&mut commands, grid_entity, grid, token, 0.0);
        }
    }

    for (entity, mut cell, parts) in &mut cells {
        let Some(next) = view.cells.iter().find(|next| next.coord == cell.coord) else {
            continue;
        };
        if next.cell != cell.shown {
            cell.shown = next.cell;
            let delay = flip_delay(cell.coord, &hops);
            commands
                .entity(entity)
                .insert(Flip::to(cell_color(next.cell), delay));
        }
        show_cell(&mut commands, parts, next);
    }

    // A win: the winner's cells bounce, once the last flips settle.
    if game.result() != *shown_result {
        *shown_result = game.result();
        if let Some(GameResult::Winner { side, .. }) = game.result() {
            let settled = hops.iter().map(|hop| hop.duration).fold(0.0, f32::max) + 0.3;
            for (entity, cell, _) in &cells {
                if cell.shown.side() == Some(side) {
                    commands
                        .entity(entity)
                        .insert(Bounce::after(settled + wave_delay(cell.coord)));
                }
            }
        }
    }
}

/// A token's move this update.
struct Path {
    from: Coord,
    /// Length in cells.
    span: f32,
    duration: f32,
}

/// When a changed cell flips: as the hopping token passes over it, or,
/// with nothing hopping (a new deal), in a wave across the board.
fn flip_delay(coord: Coord, hops: &[Path]) -> f32 {
    if hops.is_empty() {
        return wave_delay(coord);
    }
    hops.iter()
        .map(|hop| hop.duration * (distance(hop.from, coord) / hop.span).min(1.0))
        .fold(f32::INFINITY, f32::min)
}

/// A diagonal wave from the top-left corner.
fn wave_delay(coord: Coord) -> f32 {
    WAVE_STEP * (coord.x + coord.y) as f32
}

fn distance(a: Coord, b: Coord) -> f32 {
    vec2(a.x as f32, a.y as f32).distance(vec2(b.x as f32, b.y as f32))
}

/// A token node's `left`/`top`, in percent.
fn position_of(node: &Node) -> Vec2 {
    let percent = |val: Val| match val {
        Val::Percent(value) => value,
        _ => 0.0,
    };
    vec2(percent(node.left), percent(node.top))
}

fn cell_color(cell: Cell) -> Color {
    cell.side().map_or(palette::EMPTY_CELL, side_color)
}

fn visible_if(visible: bool) -> Visibility {
    if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn tap_cell(tap: On<Tap>, cells: Query<&BoardCell>, mut play: ResMut<Match>) {
    if let Ok(cell) = cells.get(tap.entity) {
        play.click(cell.coord);
    }
}
