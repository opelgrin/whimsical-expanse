//! A plain RGBA image: what atlases and source assets are made of.

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
