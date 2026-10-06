//! The look: colors, fonts and text styles.

use bevy::prelude::*;
use hibedi_core::Side;

pub fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(palette::BACKGROUND))
        .add_systems(PreStartup, load_fonts);
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
    pub const SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
    /// Jump landings, glowing tokens, primary buttons.
    pub const GOLD: Color = Color::srgb_u8(194, 137, 38);
    /// Placement cells and the token currently jumping.
    pub const CYAN: Color = Color::srgb_u8(33, 139, 154);
    /// Marks what belongs to the viewer online.
    pub const OWN_EDGE: Color = Color::srgb_u8(245, 242, 232);
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
