//! Card suit symbols, drawn when the game starts rather than shipped as
//! images: white shapes on transparent ground, tinted where they are
//! shown. Each is described by whether a point lies inside it, and
//! rasterized with supersampling for smooth edges.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Pixels per side of a symbol's image.
const SIZE: u32 = 192;
/// Samples per pixel along each axis.
const SAMPLES: u32 = 4;

/// The symbols, as images.
#[derive(Clone)]
pub struct Symbols {
    pub heart: Handle<Image>,
    pub club: Handle<Image>,
}

impl Symbols {
    pub fn draw(images: &mut Assets<Image>) -> Self {
        Self {
            heart: images.add(rasterize(heart)),
            club: images.add(rasterize(club)),
        }
    }
}

/// An image of the shape `inside` describes over the square from -1 to
/// 1 each way (y up).
fn rasterize(inside: fn(Vec2) -> bool) -> Image {
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for row in 0..SIZE {
        for column in 0..SIZE {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let u = (column as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / SIZE as f32;
                    let v = (row as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / SIZE as f32;
                    if inside(vec2(u * 2.0 - 1.0, 1.0 - v * 2.0)) {
                        hits += 1;
                    }
                }
            }
            let alpha = (255 * hits / (SAMPLES * SAMPLES)) as u8;
            data.extend_from_slice(&[255, 255, 255, alpha]);
        }
    }
    Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// ♥: two round lobes above a point (Inigo Quilez's heart).
fn heart(p: Vec2) -> bool {
    // The heart spans about 1.2 across and 1.1 up from its point at the
    // origin; centered and scaled into the square.
    let p = vec2(p.x.abs(), p.y + 0.55) * 0.62 / 0.6;
    if p.x + p.y > 1.0 {
        return (p - vec2(0.25, 0.75)).length() < std::f32::consts::SQRT_2 / 4.0;
    }
    let to_top = (p - vec2(0.0, 1.0)).length_squared();
    let to_diagonal = (p - 0.5 * (p.x + p.y).max(0.0) * Vec2::ONE).length_squared();
    to_top.min(to_diagonal).sqrt() * (p.x - p.y).signum() < 0.0
}

/// ♣: three round leaves on a flaring stem.
fn club(p: Vec2) -> bool {
    let p = p * 1.18;
    let leaf = |center: Vec2| (p - center).length() < 0.29;
    let leaves = leaf(vec2(0.0, 0.42)) || leaf(vec2(-0.33, 0.0)) || leaf(vec2(0.33, 0.0));
    let core = (p - vec2(0.0, 0.12)).length() < 0.16;
    // The stem flares from the middle down to its foot.
    let stem = (-0.82..0.1).contains(&p.y) && p.x.abs() < 0.05 + (0.1 - p.y) * 0.3;
    leaves || core || stem
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shapes fit their square and fill a fair part of it.
    #[test]
    fn symbols_fit_their_images() {
        for shape in [heart as fn(Vec2) -> bool, club] {
            let mut inside = 0;
            for y in 0..=40 {
                for x in 0..=40 {
                    let p = vec2(x as f32 / 20.0 - 1.0, y as f32 / 20.0 - 1.0);
                    let edge = x == 0 || y == 0 || x == 40 || y == 40;
                    if shape(p) {
                        assert!(!edge, "the shape reaches the edge at {p}");
                        inside += 1;
                    }
                }
            }
            assert!(inside > 41 * 41 / 8, "the shape is too small");
        }
    }
}
