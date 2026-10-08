//! The look: colors, fonts and text styles, and the board's [`Theme`].

use bevy::prelude::*;
use bevy::ui::{ColorStop, RadialGradient, RadialGradientShape, UiPosition};
use hibedi_core::{Cell, Side};

use super::symbols::Symbols;
use crate::settings::{Settings, ThemeKind};

pub fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(palette::BACKGROUND))
        .add_systems(PreStartup, (load_fonts, draw_symbols))
        .add_systems(
            PreUpdate,
            follow_settings.run_if(resource_changed::<Settings>),
        );
}

pub mod palette {
    use bevy::color::Color;

    pub const BACKGROUND: Color = Color::srgb_u8(17, 21, 30);
    /// Card surfaces.
    pub const PANEL: Color = Color::srgb_u8(31, 38, 51);
    /// Card borders, neutral buttons, highlighted tiles.
    pub const RAISED: Color = Color::srgb_u8(67, 77, 96);
    pub const TEXT: Color = Color::WHITE;
    pub const MUTED: Color = Color::srgb_u8(169, 178, 191);

    pub const EMPTY_CELL: Color = Color::srgb_u8(228, 224, 210);
    pub const RED_CELL: Color = Color::srgb_u8(211, 79, 76);
    pub const GREEN_CELL: Color = Color::srgb_u8(53, 158, 108);
    /// Dims the cells the running chain has crossed.
    pub const USED_CELL_OVERLAY: Color = Color::srgba_u8(43, 48, 57, 185);
    pub const TOKEN_FILL: Color = Color::srgb_u8(24, 31, 43);
    pub const TOKEN_EDGE: Color = Color::srgb_u8(239, 236, 220);
    /// Tokens that jumped this turn and are done for it.
    pub const SPENT_FILL: Color = Color::srgb_u8(70, 74, 82);
    pub const SPENT_EDGE: Color = Color::srgb_u8(128, 130, 132);
    pub const SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
    /// Jump landings, glowing tokens, primary buttons.
    pub const GOLD: Color = Color::srgb_u8(194, 137, 38);
    /// Placement cells and the token currently jumping.
    pub const CYAN: Color = Color::srgb_u8(33, 139, 154);
    /// Marks what belongs to the viewer online.
    pub const OWN_EDGE: Color = Color::srgb_u8(245, 242, 232);
    /// Key labels on (empty) cells.
    pub const KEY: Color = Color::srgba_u8(24, 31, 43, 150);
    /// Lines sketching moves.
    pub const SKETCH: Color = Color::srgba_u8(84, 120, 230, 190);
}

/// The board's look, as the settings choose it: how cells and tokens
/// appear. Everything else on screen keeps the [`palette`].
#[derive(Resource, Clone)]
pub struct Theme {
    pub kind: ThemeKind,
    empty: Face,
    red: Face,
    green: Face,
    /// Key labels, which stand on empty cells.
    pub key: Color,
    /// Frames the viewer's own cells online.
    pub own_edge: Color,
    stones: Stones,
}

/// What a cell shows: a color, and maybe a symbol on it.
#[derive(Clone, PartialEq)]
pub struct Face {
    pub color: Color,
    pub symbol: Option<(Handle<Image>, Color)>,
}

/// How tokens look.
#[derive(Clone, Copy)]
enum Stones {
    /// Flat discs with a light rim.
    Flat,
    /// Shaded like Go stones, without a rim.
    Go,
}

/// A token's disc.
pub struct Stone {
    pub fill: Color,
    /// Shading over the fill, if any.
    pub shading: BackgroundGradient,
    pub rim: Color,
    /// In percent of the token's cell.
    pub rim_width: f32,
}

/// Highlights the jumping token whatever the theme.
const JUMPING_RIM: f32 = 7.0;

impl Theme {
    fn new(kind: ThemeKind, symbols: &Symbols) -> Self {
        let plain = |color| Face {
            color,
            symbol: None,
        };
        match kind {
            ThemeKind::Classic => Self {
                kind,
                empty: plain(palette::EMPTY_CELL),
                red: plain(palette::RED_CELL),
                green: plain(palette::GREEN_CELL),
                key: palette::KEY,
                own_edge: palette::OWN_EDGE,
                stones: Stones::Flat,
            },
            ThemeKind::Cards => {
                let card = |symbol: &Handle<Image>, tint| Face {
                    color: cards::CARD,
                    symbol: Some((symbol.clone(), tint)),
                };
                Self {
                    kind,
                    empty: plain(cards::FELT),
                    red: card(&symbols.heart, cards::HEART),
                    green: card(&symbols.club, cards::CLUB),
                    key: cards::KEY,
                    own_edge: palette::BACKGROUND,
                    stones: Stones::Go,
                }
            }
        }
    }

    pub fn face(&self, cell: Cell) -> Face {
        match cell {
            Cell::Empty => self.empty.clone(),
            Cell::Red => self.red.clone(),
            Cell::Green => self.green.clone(),
        }
    }

    /// The disc of a token: the one jumping, a spent one, or another.
    pub fn stone(&self, jumping: bool, spent: bool) -> Stone {
        let flat = BackgroundGradient::default;
        let (fill, shading, rim, rim_width) = match self.stones {
            Stones::Flat if spent => (palette::SPENT_FILL, flat(), palette::SPENT_EDGE, 3.0),
            Stones::Flat => (palette::TOKEN_FILL, flat(), palette::TOKEN_EDGE, 3.0),
            Stones::Go => {
                // Lit from the top left: a soft highlight, falling off
                // into the stone's body.
                let [light, body, shade] = if spent {
                    cards::SPENT_STONE
                } else {
                    cards::STONE
                };
                let shading = BackgroundGradient::from(RadialGradient::new(
                    UiPosition::anchor(vec2(-0.18, -0.22)),
                    RadialGradientShape::FarthestCorner,
                    vec![
                        ColorStop::percent(light, 0),
                        ColorStop::percent(body, 38),
                        ColorStop::percent(shade, 100),
                    ],
                ));
                (body, shading, Color::NONE, 0.0)
            }
        };
        if jumping {
            Stone {
                fill,
                shading,
                rim: palette::CYAN,
                rim_width: JUMPING_RIM,
            }
        } else {
            Stone {
                fill,
                shading,
                rim,
                rim_width,
            }
        }
    }
}

/// The Cards theme's colors.
mod cards {
    use bevy::color::Color;

    /// Empty cells: a dark table.
    pub const FELT: Color = Color::srgb_u8(46, 58, 78);
    pub const CARD: Color = Color::srgb_u8(248, 245, 236);
    pub const HEART: Color = Color::srgb_u8(204, 46, 58);
    pub const CLUB: Color = Color::srgb_u8(28, 128, 78);
    pub const KEY: Color = Color::srgba_u8(235, 238, 245, 150);
    /// Light, body and shade of a stone.
    pub const STONE: [Color; 3] = [
        Color::srgb_u8(112, 116, 124),
        Color::srgb_u8(30, 32, 37),
        Color::srgb_u8(6, 6, 8),
    ];
    pub const SPENT_STONE: [Color; 3] = [
        Color::srgb_u8(190, 192, 196),
        Color::srgb_u8(120, 123, 129),
        Color::srgb_u8(82, 84, 90),
    ];
}

/// The symbols cards show, drawn once.
#[derive(Resource)]
struct SymbolImages(Symbols);

fn draw_symbols(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    settings: Res<Settings>,
) {
    let symbols = Symbols::draw(&mut images);
    commands.insert_resource(Theme::new(settings.theme, &symbols));
    commands.insert_resource(SymbolImages(symbols));
}

fn follow_settings(settings: Res<Settings>, symbols: Res<SymbolImages>, mut theme: ResMut<Theme>) {
    if theme.kind != settings.theme {
        *theme = Theme::new(settings.theme, &symbols.0);
    }
}

pub fn side_color(side: Side) -> Color {
    match side {
        Side::Red => palette::RED_CELL,
        Side::Green => palette::GREEN_CELL,
    }
}

/// Embedded UI fonts (SIL OFL, see `assets/OFL.txt`). Atkinson
/// Hyperlegible is a legibility-first typeface.
#[derive(Resource)]
pub struct Fonts {
    regular: Handle<Font>,
    bold: Handle<Font>,
}

fn load_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let mut load = |bytes: &[u8]| fonts.add(Font::from_bytes(bytes.to_vec()));
    commands.insert_resource(Fonts {
        regular: load(include_bytes!(
            "../../assets/AtkinsonHyperlegible-Regular.ttf"
        )),
        bold: load(include_bytes!("../../assets/AtkinsonHyperlegible-Bold.ttf")),
    });
}

#[derive(Clone, Copy)]
pub struct TextStyle {
    pub bold: bool,
    pub size: f32,
    pub color: Color,
}

impl TextStyle {
    pub const fn regular(size: f32, color: Color) -> Self {
        Self {
            bold: false,
            size,
            color,
        }
    }

    pub const fn bold(size: f32, color: Color) -> Self {
        Self {
            bold: true,
            size,
            color,
        }
    }
}

impl Fonts {
    /// A text node showing `text` in `style`.
    pub fn text(&self, text: impl Into<String>, style: TextStyle) -> impl Bundle {
        (Text::new(text), self.font(style), TextColor(style.color))
    }

    pub fn font(&self, style: TextStyle) -> TextFont {
        let font = if style.bold {
            &self.bold
        } else {
            &self.regular
        };
        TextFont {
            font: font.clone().into(),
            font_size: FontSize::Px(style.size),
            ..default()
        }
    }
}
