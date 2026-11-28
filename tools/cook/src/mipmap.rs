//! Offline mipmap generation: halves an atlas page in linear, premultiplied
//! RGBA so transparent pixels never bleed dark fringes into adjacent edges.

use expanse_assets::{Image, Rgba, TRANSPARENT};

/// Downsamples `image` to half its width and height, averaging each 2×2 block
/// in linear premultiplied RGBA.
///
/// # Panics
///
/// Panics if either dimension is odd or zero.
#[must_use]
pub(crate) fn downsample(image: &Image) -> Image {
    assert!(
        image.width() > 0
            && image.height() > 0
            && image.width().is_multiple_of(2)
            && image.height().is_multiple_of(2),
        "cannot halve a {}×{} image",
        image.width(),
        image.height()
    );
    let (width, height) = (image.width() / 2, image.height() / 2);
    let mut pixels = Vec::with_capacity(
        usize::try_from(u64::from(width) * u64::from(height)).expect("pixels fit in memory"),
    );
    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = (x * 2, y * 2);
            pixels.push(average_2x2([
                image.get(sx, sy),
                image.get(sx + 1, sy),
                image.get(sx, sy + 1),
                image.get(sx + 1, sy + 1),
            ]));
        }
    }
    Image::from_pixels(width, height, pixels).expect("one pixel per position")
}

/// Averages four sRGB straight-alpha pixels in linear premultiplied space.
fn average_2x2(block: [Rgba; 4]) -> Rgba {
    let mut sum_r = 0.0_f32;
    let mut sum_g = 0.0_f32;
    let mut sum_b = 0.0_f32;
    let mut sum_a = 0.0_f32;
    for [r, g, b, a] in block {
        let alpha = f32::from(a) / 255.0;
        sum_r += srgb_to_linear(r) * alpha;
        sum_g += srgb_to_linear(g) * alpha;
        sum_b += srgb_to_linear(b) * alpha;
        sum_a += alpha;
    }
    if sum_a == 0.0 {
        return TRANSPARENT;
    }
    [
        linear_to_srgb(sum_r / sum_a),
        linear_to_srgb(sum_g / sum_a),
        linear_to_srgb(sum_b / sum_a),
        to_u8(sum_a * 0.25),
    ]
}

fn srgb_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(linear: f32) -> u8 {
    let clamped = linear.clamp(0.0, 1.0);
    let srgb = if clamped <= 0.003_130_8 {
        clamped * 12.92
    } else {
        1.055 * clamped.powf(1.0 / 2.4) - 0.055
    };
    to_u8(srgb)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0.0..=1.0 before scaling to 0..=255"
)]
fn to_u8(unit: f32) -> u8 {
    (unit.clamp(0.0, 1.0) * 255.0).round() as u8
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches straight-alpha averaging that blends transparent `[0, 0, 0, 0]`
    /// pixels' zero RGB into an opaque edge, producing a dark halo in the
    /// mipmap level.
    #[test]
    fn downsampling_preserves_colour_next_to_transparent_pixels() {
        let mut image = Image::new(2, 2);
        image.set(0, 0, [240, 120, 60, 255]);
        let mip = downsample(&image);
        assert_eq!((mip.width(), mip.height()), (1, 1));
        assert_eq!(mip.get(0, 0), [240, 120, 60, 64]);
    }
}
