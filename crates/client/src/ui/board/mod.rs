//! The board: a grid of cells with tokens on top, following the match.
//!
//! The entities only ever follow a [`BoardView`] of the current game.
//! Whenever the match changes, the new view is compared with what is
//! shown and the differences animate: tokens hop to where they moved,
//! crossed cells flip as the hopping token passes over them, and a new
//! deal flips the board over in a wave. Clicks on cells go to the match.
//!
//! Cells sit where the [`Lattice`] puts them, so a square grid and a hex
//! lattice are laid out the same way, and a game dealt on another grid
//! rebuilds the board.

mod hex;
pub(crate) mod lattice;
mod view;

use bevy::prelude::*;
use hibedi_core::{Cell, Coord, Game, GameResult, Grid};

use super::Screen;
use super::anim::{Animate, Bounce, ColorTo, Flip, Hop, Pulse, ScaleTo};
use super::layout::Content;
use super::tap::{Tap, Tappable};
use super::theme::{palette, side_color};
use crate::play::Match;
use hex::Hexagons;
use lattice::Lattice;
use view::{BoardView, CellView, Halo, Hint, TokenView};

pub fn plugin(app: &mut App) {
    app.add_plugins(hex::plugin).add_systems(
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
/// How far a cell's corners are rounded, in percent, on a square grid.
const CORNER: f32 = 8.0;

/// The board's frame; the layout sizes it.
#[derive(Component)]
pub struct BoardFrame;

/// The grid of cells inside the frame; cells and tokens are placed on it
/// in percent of its size.
#[derive(Component)]
struct BoardGrid {
    lattice: Lattice,
    /// The frame to replace when the game moves to another grid.
    frame: Entity,
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
pub fn spawn(
    mut commands: Commands,
    play: Res<Match>,
    hexes: Res<Hexagons>,
    content: Single<Entity, With<Content>>,
) {
    let game = play.game().expect("a game is being played");
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
    build(&mut commands, frame, game, &play, &hexes);
}

/// Fills `frame` with the cells and tokens `game` is played on, as `play`
/// shows it.
fn build(commands: &mut Commands, frame: Entity, game: &Game, play: &Match, hexes: &Hexagons) {
    let view = BoardView::of(game, play.shows_hints(), play.my_side());
    let lattice = Lattice::new(game.board().grid());

    let grid = commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ChildOf(frame),
        ))
        .id();

    for cell in &view.cells {
        spawn_cell(commands, grid, &lattice, hexes, cell);
    }
    for token in &view.tokens {
        spawn_token(commands, grid, &lattice, token, wave_delay(token.position));
    }
    commands.entity(grid).insert(BoardGrid { lattice, frame });
}

fn spawn_cell(
    commands: &mut Commands,
    grid: Entity,
    lattice: &Lattice,
    hexes: &Hexagons,
    view: &CellView,
) {
    let (at, size) = lattice.inlay(view.coord);
    let cell = commands
        .spawn((
            BoardCell {
                coord: view.coord,
                shown: view.cell,
            },
            Node {
                border_radius: BorderRadius::all(percent(CORNER)),
                ..box_at(at, size)
            },
            ScaleTo::appear(wave_delay(view.coord)),
            Tappable,
            ChildOf(grid),
        ))
        .observe(tap_cell)
        .id();
    // Bevy's UI has rounded corners and nothing with six, so a hex cell
    // is a tinted hexagon image and a square cell a tinted background.
    match lattice.grid() {
        Grid::Square { .. } => {
            commands
                .entity(cell)
                .insert(BackgroundColor(cell_color(view.cell)));
        }
        Grid::Hex { .. } => {
            commands
                .entity(cell)
                .insert(hexagon(hexes.fill.clone(), cell_color(view.cell)));
        }
    }

    let overlay = match lattice.grid() {
        Grid::Square { .. } => part(commands, cell, centered(lattice, 100.0)),
        Grid::Hex { .. } => part(commands, cell, hexagon(hexes.fill.clone(), Color::NONE)),
    };
    let frame = match lattice.grid() {
        Grid::Square { .. } => {
            let node = part(
                commands,
                cell,
                Node {
                    border: UiRect::all(percent(3)),
                    border_radius: BorderRadius::all(percent(6)),
                    ..centered(lattice, 86.0)
                },
            );
            commands
                .entity(node)
                .insert(BorderColor::all(palette::OWN_EDGE));
            node
        }
        // A hexagon outlined on itself: the same texture, empty inside.
        Grid::Hex { .. } => part(
            commands,
            cell,
            hexagon(hexes.ring.clone(), palette::OWN_EDGE),
        ),
    };
    let ring = part(
        commands,
        cell,
        Node {
            border: UiRect::all(percent(5)),
            border_radius: BorderRadius::MAX,
            ..centered(lattice, 56.0)
        },
    );
    let dot = part(
        commands,
        ring,
        (
            Node {
                border_radius: BorderRadius::MAX,
                ..centered(lattice, 36.0)
            },
            // The dot shows when its ring does, and a ring only reaches
            // a child that has a visibility of its own.
            Visibility::Inherited,
        ),
    );
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
    lattice: &Lattice,
    view: &TokenView,
    delay: f32,
) {
    let token = commands
        .spawn((
            BoardToken { view: *view },
            box_at(lattice.anchor(view.position), lattice.cell()),
            ScaleTo::appear(delay),
            ChildOf(grid_entity),
        ))
        .id();
    let halo = part(
        commands,
        token,
        Node {
            border: UiRect::all(percent(2.5)),
            border_radius: BorderRadius::MAX,
            ..centered(lattice, 76.0)
        },
    );
    let disc = part(
        commands,
        token,
        (
            BackgroundColor(palette::TOKEN_FILL),
            BoxShadow::new(palette::SHADOW, px(0), percent(6), px(0), percent(8)),
            Node {
                border: UiRect::all(percent(3)),
                border_radius: BorderRadius::MAX,
                ..centered(lattice, 54.0)
            },
        ),
    );

    let parts = TokenParts { halo, disc };
    show_token(commands, &parts, lattice, view);
    commands.entity(token).insert(parts);
}

/// Applies a token's rim and halo.
fn show_token(commands: &mut Commands, parts: &TokenParts, lattice: &Lattice, view: &TokenView) {
    // The token jumping wears a thick cyan rim, the others a thin light
    // one (in percent of the token's cell).
    let (rim, rim_color) = if view.jumping {
        (7.0, palette::CYAN)
    } else {
        (3.0, palette::TOKEN_EDGE)
    };
    commands.entity(parts.disc).insert(Node {
        border: UiRect::all(percent(rim)),
        ..centered(lattice, 54.0)
    });
    commands
        .entity(parts.disc)
        .insert(BorderColor::all(rim_color));

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

/// An absolutely positioned node `size` percent across, at `at`: both
/// in percent of the parent.
fn box_at(at: Vec2, size: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: percent(at.x),
        top: percent(at.y),
        width: percent(size.x),
        height: percent(size.y),
        ..default()
    }
}

/// An absolutely positioned node `size` percent of a cell's width,
/// centered on it.
fn centered(lattice: &Lattice, size: f32) -> Node {
    let size = lattice.square(size);
    box_at((Vec2::splat(100.0) - size) / 2.0, size)
}

/// A hexagon texture, in `color`, filling its node.
fn hexagon(image: Handle<Image>, color: Color) -> ImageNode {
    ImageNode {
        image,
        color,
        // A texture is exactly a hexagon's own box, so filling the
        // cell's box draws the hexagon at the right proportions.
        image_mode: NodeImageMode::Stretch,
        ..default()
    }
}

/// Spawns a decoration inside `parent`.
fn part(commands: &mut Commands, parent: Entity, node: impl Bundle) -> Entity {
    commands.spawn((node, ChildOf(parent))).id()
}

/// Brings the board up to date with the match, animating every change.
fn follow_match(
    mut commands: Commands,
    play: Res<Match>,
    hexes: Res<Hexagons>,
    grid: Single<(Entity, &BoardGrid)>,
    mut cells: Query<(Entity, &mut BoardCell, &CellParts)>,
    tokens: Query<(Entity, &BoardToken, &TokenParts, &Node)>,
    mut shown_result: Local<Option<GameResult>>,
) {
    let Some(game) = play.game() else {
        return;
    };
    let lattice = Lattice::new(game.board().grid());
    let (grid_entity, board) = *grid;

    // A game on another grid is another board: the cells shown are the
    // wrong shape in the wrong places, so they are replaced whole.
    if board.lattice.grid() != lattice.grid() {
        commands.entity(grid_entity).despawn();
        build(&mut commands, board.frame, game, &play, &hexes);
        return;
    }

    let view = BoardView::of(game, play.shows_hints(), play.my_side());

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
            let span = lattice.distance(from, next.position);
            let duration = HOP_BASE + HOP_PER_CELL * span;
            hops.push(Path {
                from,
                span,
                duration,
            });
            commands.entity(entity).remove::<ScaleTo>().insert(Hop::new(
                position_of(node),
                lattice.anchor(next.position),
                duration,
            ));
        }
        commands.entity(entity).insert(BoardToken { view: *next });
        show_token(&mut commands, parts, &lattice, next);
    }
    for token in &view.tokens {
        if !tokens
            .iter()
            .any(|(_, shown, ..)| shown.view.id == token.id)
        {
            spawn_token(&mut commands, grid_entity, &lattice, token, 0.0);
        }
    }

    for (entity, mut cell, parts) in &mut cells {
        let Some(next) = view.cells.iter().find(|next| next.coord == cell.coord) else {
            continue;
        };
        if next.cell != cell.shown {
            cell.shown = next.cell;
            let delay = flip_delay(&lattice, cell.coord, &hops);
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
fn flip_delay(lattice: &Lattice, coord: Coord, hops: &[Path]) -> f32 {
    if hops.is_empty() {
        return wave_delay(coord);
    }
    hops.iter()
        .map(|hop| hop.duration * (lattice.distance(hop.from, coord) / hop.span).min(1.0))
        .fold(f32::INFINITY, f32::min)
}

/// A diagonal wave from the top-left corner.
fn wave_delay(coord: Coord) -> f32 {
    WAVE_STEP * (coord.x + coord.y) as f32
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
