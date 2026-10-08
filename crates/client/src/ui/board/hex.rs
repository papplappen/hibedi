//! The hexagon a hex board is drawn with.
//!
//! Bevy's UI draws rounded rectangles and nothing with six corners, so
//! a hex cell is a texture: generated once at startup, then tinted per
//! cell like any other colored node. Two are needed, a solid one for a
//! cell and an outline for the marker on a cell that is the viewer's.
//!
//! The hexagon fills its texture exactly, so stretching the texture over
//! a cell's box draws it at the right proportions.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// The hexagon textures a hex board is drawn with.
#[derive(Resource)]
pub struct Hexagons {
    /// A solid hexagon, tinted to a cell's color.
    pub fill: Handle<Image>,
    /// The same hexagon as an outline.
    pub ring: Handle<Image>,
}

/// The distance from the middle of the hexagon to a corner, in pixels.
const RADIUS: f32 = 150.0;
/// The hexagon's own box: √3 across for a radius, twice the radius tall.
/// The texture fills it exactly, so filling a cell's box with it draws
/// the hexagon as large as the cell allows.
const WIDTH: u32 = 260;
const HEIGHT: u32 = 300;
/// The distance from the middle to a side, which is shorter.
const APOTHEM: f32 = 0.866_025_4 * RADIUS;
/// How deep the outline is, in pixels.
const OUTLINE: f32 = 12.0;
/// The slant of a hexagon's sides: the root of three.
const SQRT_3: f32 = 1.732_050_8;

pub fn plugin(app: &mut App) {
    app.add_systems(PreStartup, generate);
}

fn generate(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(Hexagons {
        fill: images.add(texture(filled)),
        ring: images.add(texture(outlined)),
    });
}

/// A texture of a hexagon, drawn by `alpha` at each pixel, measured from
/// the middle.
fn texture(alpha: fn(f32, f32) -> f32) -> Image {
    let mut data = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let at = (
                x as f32 + 0.5 - WIDTH as f32 / 2.0,
                y as f32 + 0.5 - HEIGHT as f32 / 2.0,
            );
            let alpha = (alpha(at.0, at.1) * 255.0).round().clamp(0.0, 255.0) as u8;
            // White, so a tint reaches the node as it comes.
            data.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

/// The hexagon itself: covered `depth` inside its edges, feathering
/// over the last pixel.
fn filled(x: f32, y: f32) -> f32 {
    inside(x, y, 0.0)
}

/// The hexagon's rim: everything from its edge inward [`OUTLINE`], and
/// hollow beyond, feathered along both.
fn outlined(x: f32, y: f32) -> f32 {
    (inside(x, y, 0.0) - inside(x, y, OUTLINE)).clamp(0.0, 1.0)
}

/// How much of the hexagon is within `depth` of `(x, y)`.
fn inside(x: f32, y: f32, depth: f32) -> f32 {
    (0.5 - outside(x, y) - depth).clamp(0.0, 1.0)
}

/// How far `(x, y)`, measured from the middle, is outside a pointy-top
/// hexagon: negative inside it, and an exact distance either way,
/// because the hexagon is three strips' meeting — one upright, two at
/// sixty degrees — and every one of them has a unit normal.
fn outside(x: f32, y: f32) -> f32 {
    let (x, y) = (x.abs(), y.abs());
    let slanted = (x + SQRT_3 * y - SQRT_3 * RADIUS) / 2.0;
    let other = (x - SQRT_3 * y - SQRT_3 * RADIUS) / 2.0;
    (x - APOTHEM).max(slanted).max(other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_texture_is_sized_for_a_hexagon() {
        // A hexagon is twice its radius tall and √3 across it, and the
        // texture is that box: not a square, or the hexagon would be
        // drawn smaller than the cell it fills.
        for (measured, expected) in [
            (WIDTH as f32, SQRT_3 * RADIUS),
            (HEIGHT as f32, 2.0 * RADIUS),
        ] {
            assert!(
                (measured - expected).abs() <= 1.0,
                "a {RADIUS} radius hexagon is {expected} across, not {measured}"
            );
        }
    }

    #[test]
    fn the_hexagons_corners_are_its_corners() {
        // Each corner of the hexagon is on its edge, and each side is
        // at the apothem.
        assert_eq!(filled(0.0, RADIUS), 0.5);
        assert_eq!(filled(APOTHEM, 0.0), 0.5);
        assert_eq!(filled(APOTHEM, RADIUS / 2.0), 0.5);

        // The middle is well covered.
        assert_eq!(filled(0.0, 0.0), 1.0);
        // The hexagon fills its texture: its corners reach the top and
        // bottom edges and its sides sit just inside the left and
        // right ones, so nothing of it is cut off.
        assert_eq!(filled(0.0, HEIGHT as f32 / 2.0), 0.5);
        assert!(filled(WIDTH as f32 / 2.0, 0.0) < 0.5);
        // And the texture's own corners are well outside it.
        assert_eq!(filled(WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0), 0.0);
    }

    #[test]
    fn the_outline_is_the_hexagons_rim() {
        let rim = (0.0, RADIUS - OUTLINE / 2.0);

        assert_eq!(outlined(rim.0, rim.1), 1.0);
        // Hollow in the middle and past the rim.
        assert_eq!(outlined(0.0, 0.0), 0.0);
        assert_eq!(outlined(0.0, RADIUS + 2.0), 0.0);
    }

    #[test]
    fn the_texture_is_white_where_it_is_covered() {
        let image = texture(|_, _| 1.0);
        assert_eq!(image.width(), WIDTH);
        assert_eq!(image.height(), HEIGHT);
        let data = image.data.expect("a texture has pixels");
        assert_eq!(data.len(), (WIDTH * HEIGHT * 4) as usize);
        assert_eq!(data[..4], [255, 255, 255, 255]);

        let image = texture(filled);
        let data = image.data.expect("a texture has pixels");
        let middle = ((HEIGHT / 2 * WIDTH + WIDTH / 2) * 4) as usize;
        assert_eq!(data[middle + 3], 255, "the middle is covered");
        assert_eq!(data[0..4], [255, 255, 255, 0], "the corner is not");
    }
}
