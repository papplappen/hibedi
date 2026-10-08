//! The board: a grid of cells with tokens on top, following the match.
//!
//! The entities only ever follow a [`BoardView`] of the current game.
//! Whenever the match changes, the new view is compared with what is
//! shown and the differences animate: tokens hop to where they moved,
//! crossed cells flip as the hopping token passes over them, and a new
//! deal flips the board over in a wave. Clicks on cells go to the match.
//!
//! On top of the game, the board shows the [`Sketch`] of moves being
//! tried out: its lines, and the cells they flip.

mod sketch;
mod view;

use bevy::prelude::*;
use hibedi_core::{Cell, Coord, Game, GameResult};

use super::Screen;
use super::anim::{Animate, Bounce, ColorTo, FaceSymbol, Flip, Hop, Pulse, ScaleTo, show_face};
use super::layout::Content;
use super::tap::{Tap, Tappable};
use super::theme::{Fonts, TextStyle, Theme, palette};
use crate::play::Match;
use crate::settings::ThemeKind;
use sketch::{Line, Sketch};
use view::{BoardView, CellView, Halo, Hint, TokenView, Viewer};

pub fn plugin(app: &mut App) {
    app.add_plugins(sketch::plugin).add_systems(
        Update,
        (
            follow_match.run_if(
                resource_changed::<Match>
                    .or_eager(resource_changed::<Sketch>)
                    .or_eager(resource_changed::<Theme>),
            ),
            show_sketch.run_if(resource_changed::<Sketch>),
            size_keys,
        )
            .run_if(in_state(Screen::Playing))
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
/// Key labels' size, in cells.
const KEY_SIZE: f32 = 0.16;
/// A face's symbol's size, in percent of its cell.
const SYMBOL_SIZE: f32 = 80.0;
/// Sketched lines' thickness, and the dot at their end, in cells.
const SKETCH_LINE: f32 = 0.18;
const SKETCH_DOT: f32 = 0.4;

/// The board's frame; the layout sizes it.
#[derive(Component)]
pub struct BoardFrame;

/// The grid of cells inside the frame; tokens are positioned on it in
/// percent of its size.
#[derive(Component, PartialEq)]
struct BoardGrid {
    columns: usize,
    rows: usize,
}

impl BoardGrid {
    /// The grid a game's board needs: its cells, outer ring included.
    fn of(game: &Game) -> Self {
        Self {
            columns: game.board().storage_width(),
            rows: game.board().storage_height(),
        }
    }

    /// The `left`/`top` of a cell, in percent of the grid.
    fn position(&self, coord: Coord) -> Vec2 {
        vec2(
            coord.x as f32 * 100.0 / self.columns as f32,
            coord.y as f32 * 100.0 / self.rows as f32,
        )
    }

    /// A band `thickness` cells thick (and long beyond the centers)
    /// joining the centers of two cells, in percent of the grid.
    fn span(&self, from: Coord, to: Coord, thickness: f32) -> Rect {
        let cell = vec2(100.0 / self.columns as f32, 100.0 / self.rows as f32);
        let center = |coord: Coord| (vec2(coord.x as f32, coord.y as f32) + 0.5) * cell;
        let half = thickness / 2.0 * cell;
        Rect::from_corners(center(from) - half, center(to) + half)
    }
}

#[derive(Component)]
struct BoardCell {
    coord: Coord,
    /// The color shown, or being flipped to.
    shown: Cell,
}

/// A cell's decorations, bottom to top, over the symbol of its face
/// ([`FaceSymbol`]).
#[derive(Component)]
struct CellParts {
    /// Dims crossed cells.
    overlay: Entity,
    /// Marks the viewer's own cells.
    frame: Entity,
    /// The hint marker: a ring around a dot.
    ring: Entity,
    dot: Entity,
    /// The label of the key taking the action aimed here.
    key: Entity,
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
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    content: Single<Entity, With<Content>>,
) {
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
    build_grid(&mut commands, &fonts, &theme, frame, &play);
}

/// Builds the grid of the match's game in the frame, growing in with a
/// wave: the board's structure (its cells) follows the game's
/// dimensions; everything else follows changes ([`follow_match`]).
fn build_grid(commands: &mut Commands, fonts: &Fonts, theme: &Theme, frame: Entity, play: &Match) {
    let game = play.game().expect("a game is being played");
    let view = BoardView::of(game, viewer(play));
    let grid = BoardGrid::of(game);
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
        spawn_cell(commands, fonts, theme, grid_entity, cell);
    }
    for token in &view.tokens {
        spawn_token(
            commands,
            theme,
            grid_entity,
            &grid,
            token,
            wave_delay(token.position),
        );
    }
    commands.entity(grid_entity).insert(grid);
}

/// Who looks at the board, and what they may do.
fn viewer(play: &Match) -> Viewer {
    Viewer {
        side: play.my_side(),
        hints: play.shows_hints(),
        acts: play.can_move(),
    }
}

fn spawn_cell(
    commands: &mut Commands,
    fonts: &Fonts,
    theme: &Theme,
    grid: Entity,
    view: &CellView,
) {
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
            ScaleTo::appear(wave_delay(view.coord)),
            Tappable,
            ChildOf(grid),
        ))
        .observe(tap_cell)
        .id();
    let mut part = |node: Node| commands.spawn((node, ChildOf(cell))).id();
    let symbol = part(centered(SYMBOL_SIZE));
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
    let key = commands
        .spawn((
            KeyLabel,
            fonts.text("", TextStyle::bold(12.0, palette::KEY)),
            Node {
                position_type: PositionType::Absolute,
                right: percent(7),
                bottom: percent(2),
                ..default()
            },
            ChildOf(cell),
        ))
        .id();
    commands.entity(ring).insert(Pulse { amount: 0.06 });
    let face = show_face(commands, symbol, &theme.face(view.cell));
    commands.entity(cell).insert((face, FaceSymbol(symbol)));

    let parts = CellParts {
        overlay,
        frame,
        ring,
        dot,
        key,
    };
    show_cell(commands, theme, &parts, view);
    commands.entity(cell).insert(parts);
}

/// Applies a cell's decorations; its face is the caller's business.
fn show_cell(commands: &mut Commands, theme: &Theme, parts: &CellParts, view: &CellView) {
    let overlay = if view.used {
        palette::USED_CELL_OVERLAY
    } else {
        Color::NONE
    };
    commands.entity(parts.overlay).insert(ColorTo(overlay));
    commands
        .entity(parts.frame)
        .insert((visible_if(view.own), BorderColor::all(theme.own_edge)));

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

    let key = view.key.map(|key| key.to_uppercase().to_string());
    commands.entity(parts.key).insert((
        visible_if(key.is_some()),
        Text::new(key.unwrap_or_default()),
        TextColor(theme.key),
    ));
}

/// Marks the key labels, sized to the cells.
#[derive(Component)]
struct KeyLabel;

/// Keeps the key labels in proportion to the cells, which grow and
/// shrink with the window.
fn size_keys(
    grid: Single<(&BoardGrid, &ComputedNode)>,
    mut labels: Query<&mut TextFont, With<KeyLabel>>,
) {
    let (grid, node) = *grid;
    let cell = node.size().x * node.inverse_scale_factor() / grid.columns as f32;
    let size = FontSize::Px((KEY_SIZE * cell).round().max(9.0));
    for mut font in &mut labels {
        if font.font_size != size {
            font.font_size = size;
        }
    }
}

fn spawn_token(
    commands: &mut Commands,
    theme: &Theme,
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
            BackgroundColor(theme.stone(view.jumping, view.spent).fill),
            BoxShadow::new(palette::SHADOW, px(0), percent(6), px(0), percent(8)),
            ChildOf(token),
        ))
        .id();

    let parts = TokenParts { halo, disc };
    show_token(commands, theme, &parts, view);
    commands.entity(token).insert(parts);
}

/// Applies a token's disc and halo. The token jumping wears a thick cyan
/// rim; spent tokens fade into the board.
fn show_token(commands: &mut Commands, theme: &Theme, parts: &TokenParts, view: &TokenView) {
    let stone = theme.stone(view.jumping, view.spent);
    commands.entity(parts.disc).insert((
        Node {
            border: UiRect::all(percent(stone.rim_width)),
            border_radius: BorderRadius::MAX,
            ..centered(54.0)
        },
        BorderColor::all(stone.rim),
        ColorTo(stone.fill),
        stone.shading,
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

/// Brings the board up to date with the match and the sketch, animating
/// every change. A game of other dimensions gets a grid of its own.
fn follow_match(
    mut commands: Commands,
    play: Res<Match>,
    sketch: Res<Sketch>,
    theme: Res<Theme>,
    fonts: Res<Fonts>,
    frame: Single<Entity, With<BoardFrame>>,
    grid: Single<(Entity, &BoardGrid)>,
    mut cells: Query<(Entity, &mut BoardCell, &CellParts)>,
    tokens: Query<(Entity, &BoardToken, &TokenParts, &Node)>,
    mut shown: Local<Shown>,
) {
    let Some(game) = play.game() else {
        return;
    };
    let (grid_entity, grid) = *grid;
    if *grid != BoardGrid::of(game) {
        commands.entity(grid_entity).despawn();
        build_grid(&mut commands, &fonts, &theme, *frame, &play);
        *shown = Shown {
            result: game.result(),
            lines: sketch.lines.clone(),
            theme: Some(theme.kind),
        };
        return;
    }
    let view = BoardView::of(game, viewer(&play));

    // Tokens first: where they hop decides when the cells they cross
    // flip. Lines drawn or erased sweep over their cells the same way.
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
            let path = Path::new(token.view.position, next.position);
            let duration = path.duration;
            hops.push(path);
            commands.entity(entity).remove::<ScaleTo>().insert(Hop::new(
                position_of(node),
                grid.position(next.position),
                duration,
            ));
        }
        commands.entity(entity).insert(BoardToken { view: *next });
        show_token(&mut commands, &theme, parts, next);
    }
    for token in &view.tokens {
        if !tokens
            .iter()
            .any(|(_, shown, ..)| shown.view.id == token.id)
        {
            spawn_token(&mut commands, &theme, grid_entity, grid, token, 0.0);
        }
    }

    let sweeps: Vec<Path> = hops
        .iter()
        .copied()
        .chain(
            sketch
                .lines
                .iter()
                .filter(|line| !shown.lines.contains(line))
                .chain(
                    shown
                        .lines
                        .iter()
                        .filter(|line| !sketch.lines.contains(line)),
                )
                .map(|line| Path::new(line.from, line.to)),
        )
        .collect();
    shown.lines = sketch.lines.clone();
    // A new look turns every cell over, in a wave.
    let new_look = shown
        .theme
        .replace(theme.kind)
        .is_some_and(|kind| kind != theme.kind);

    for (entity, mut cell, parts) in &mut cells {
        let Some(next) = view.cells.iter().find(|next| next.coord == cell.coord) else {
            continue;
        };
        let shown = if sketch.flips(cell.coord) {
            next.cell.flipped()
        } else {
            next.cell
        };
        if shown != cell.shown || new_look {
            cell.shown = shown;
            let (delay, direction) = if new_look {
                (wave_delay(cell.coord), Vec2::ZERO)
            } else {
                flip_timing(cell.coord, &sweeps)
            };
            commands
                .entity(entity)
                .insert(Flip::to(theme.face(shown), delay, direction));
        }
        show_cell(&mut commands, &theme, parts, next);
    }

    // A win: the winner's cells bounce, once the last flips settle.
    if game.result() != shown.result {
        shown.result = game.result();
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

/// What the board last showed, beyond its entities.
#[derive(Default)]
struct Shown {
    result: Option<GameResult>,
    lines: Vec<Line>,
    theme: Option<ThemeKind>,
}

/// A sweep over the board this update: a token's move, or a sketched
/// line drawn or erased.
#[derive(Clone, Copy)]
struct Path {
    from: Coord,
    to: Coord,
    /// Length in cells.
    span: f32,
    duration: f32,
}

impl Path {
    fn new(from: Coord, to: Coord) -> Self {
        let span = distance(from, to);
        Self {
            from,
            to,
            span,
            duration: HOP_BASE + HOP_PER_CELL * span,
        }
    }

    /// When the sweep reaches `coord`.
    fn reach(&self, coord: Coord) -> f32 {
        if self.span == 0.0 {
            return 0.0;
        }
        self.duration * (distance(self.from, coord) / self.span).min(1.0)
    }

    /// The way the sweep goes, as a unit step along its longer axis; zero
    /// for a single cell.
    fn direction(&self) -> Vec2 {
        let (dx, dy) = (self.to.x - self.from.x, self.to.y - self.from.y);
        if dx.abs() >= dy.abs() {
            vec2(dx.signum() as f32, 0.0)
        } else {
            vec2(0.0, dy.signum() as f32)
        }
    }

    /// Whether `coord` lies on the straight run from `from` to `to`.
    fn covers(&self, coord: Coord) -> bool {
        Line {
            from: self.from,
            to: self.to,
        }
        .covers(coord)
    }
}

/// When and which way a changed cell flips: with the sweep passing over
/// it, or, with nothing sweeping (a new deal), in a wave across the
/// board.
fn flip_timing(coord: Coord, sweeps: &[Path]) -> (f32, Vec2) {
    if sweeps.is_empty() {
        return (wave_delay(coord), Vec2::ZERO);
    }
    // A sweep over the cell, else (several hops arriving at once) the
    // nearest.
    let over: Vec<&Path> = sweeps.iter().filter(|sweep| sweep.covers(coord)).collect();
    let candidates = if over.is_empty() {
        sweeps.iter().collect()
    } else {
        over
    };
    candidates
        .into_iter()
        .map(|sweep| (sweep.reach(coord), sweep.direction()))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .expect("there is a sweep")
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

/// A mark of the sketch on the board.
#[derive(Component)]
struct SketchMark;

/// Draws the sketch's lines over the board, each ending in a dot; the
/// line being drawn shows fainter.
fn show_sketch(
    mut commands: Commands,
    sketch: Res<Sketch>,
    grid: Single<(Entity, &BoardGrid)>,
    marks: Query<Entity, With<SketchMark>>,
) {
    for mark in &marks {
        commands.entity(mark).despawn();
    }
    let (grid_entity, grid) = *grid;
    let lines = sketch.lines.iter().map(|&line| (line, palette::SKETCH));
    let drawing = sketch.drawing.map(|line| {
        (
            line,
            palette::SKETCH.with_alpha(palette::SKETCH.alpha() * 0.6),
        )
    });
    for (line, color) in lines.chain(drawing) {
        let (low, high) = (
            Coord::new(line.from.x.min(line.to.x), line.from.y.min(line.to.y)),
            Coord::new(line.from.x.max(line.to.x), line.from.y.max(line.to.y)),
        );
        let bar = grid.span(low, high, SKETCH_LINE);
        let dot = grid.span(line.to, line.to, SKETCH_DOT);
        for rect in [bar, dot] {
            commands.spawn((
                SketchMark,
                Node {
                    position_type: PositionType::Absolute,
                    left: percent(rect.min.x),
                    top: percent(rect.min.y),
                    width: percent(rect.width()),
                    height: percent(rect.height()),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(color),
                // Over the tokens.
                ZIndex(1),
                ChildOf(grid_entity),
            ));
        }
    }
}
