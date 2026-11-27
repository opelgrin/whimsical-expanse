//! A plain RGBA image: what source assets hold and atlases are made of.

/// One pixel: red, green, blue and alpha, in sRGB with straight alpha.
pub type Rgba = [u8; 4];

/// A fully transparent pixel.
pub const TRANSPARENT: Rgba = [0, 0, 0, 0];

/// An image of `width × height` pixels, stored row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
}

impl Image {
    /// A transparent image.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![TRANSPARENT; pixel_count(width, height)],
        }
    }

    /// An image with these pixels, row by row, or `None` if their number is
    /// not `width × height`.
    #[must_use]
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Rgba>) -> Option<Self> {
        (pixels.len() == pixel_count(width, height)).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// The width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// The height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[Rgba] {
        &self.pixels
    }

    /// The pixel at `(x, y)`.
    ///
    /// # Panics
    ///
    /// Panics if `(x, y)` lies outside the image.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Rgba {
        self.pixels[self.index(x, y)]
    }

    /// Sets the pixel at `(x, y)`.
    ///
    /// # Panics
    ///
    /// Panics if `(x, y)` lies outside the image.
    pub fn set(&mut self, x: u32, y: u32, pixel: Rgba) {
        let index = self.index(x, y);
        self.pixels[index] = pixel;
    }

    /// Copies the `width × height` pixels at `(from_x, from_y)` in `source` to
    /// `(to_x, to_y)` in this image.
    ///
    /// # Panics
    ///
    /// Panics if either rectangle lies partly outside its image.
    #[expect(clippy::too_many_arguments, reason = "two corners and a size")]
    pub fn copy_from(
        &mut self,
        source: &Self,
        from_x: u32,
        from_y: u32,
        width: u32,
        height: u32,
        to_x: u32,
        to_y: u32,
    ) {
        for y in 0..height {
            for x in 0..width {
                self.set(to_x + x, to_y + y, source.get(from_x + x, from_y + y));
            }
        }
    }

    /// Downsamples the image to half its width and height, averaging each 2×2
    /// block in linear, premultiplied RGBA so transparent pixels never bleed
    /// dark fringes into adjacent opaque edges.
    ///
    /// # Panics
    ///
    /// Panics if either dimension is odd or zero.
    #[must_use]
    pub fn downsample(&self) -> Self {
        assert!(
            self.width > 0
                && self.height > 0
                && self.width.is_multiple_of(2)
                && self.height.is_multiple_of(2),
            "cannot halve a {}×{} image",
            self.width,
            self.height
        );
        let (width, height) = (self.width / 2, self.height / 2);
        let mut pixels = Vec::with_capacity(pixel_count(width, height));
        for y in 0..height {
            for x in 0..width {
                let (sx, sy) = (x * 2, y * 2);
                pixels.push(average_2x2([
                    self.get(sx, sy),
                    self.get(sx + 1, sy),
                    self.get(sx, sy + 1),
                    self.get(sx + 1, sy + 1),
                ]));
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn index(&self, x: u32, y: u32) -> usize {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) lies outside a {}×{} image",
            self.width,
            self.height
        );
        usize::try_from(u64::from(y) * u64::from(self.width) + u64::from(x))
            .expect("an image's pixels fit in memory")
    }
}

fn pixel_count(width: u32, height: u32) -> usize {
    usize::try_from(u64::from(width) * u64::from(height)).expect("an image's pixels fit in memory")
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
        let mip = image.downsample();
        assert_eq!((mip.width(), mip.height()), (1, 1));
        assert_eq!(mip.get(0, 0), [240, 120, 60, 64]);
    }
}
