//! The colours of the terrain: by biome, or by one field in a debug view.
//!
//! Colours are worked out once, when the palette is made, so that colouring a
//! tile each frame is a table lookup.

use expanse_terrain::{BiomeIndex, BiomeTable, Field, SEA_LEVEL, Tile};

/// What colours the terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainView {
    /// Each tile in its biome's map colour.
    Biomes,
    /// Each tile by the value of one field, on a colour ramp. Moisture and
    /// vegetation are ranked among land tiles, so water shows in one flat
    /// colour in their views.
    Field(Field),
}

/// Each colour of a field's ramp covers a run of `2^FIELD_STEP_BITS`
/// quantised values: about 0.3 °C, or 1/1024 of a rank.
const FIELD_STEP_BITS: u32 = 6;

/// The colour of a biome the palette does not know, which only a terrain
/// generated from another table can have.
const UNKNOWN_BIOME: [u8; 3] = [255, 0, 255];

/// The colour of water in the views of fields ranked among land tiles.
const WATER: [u8; 3] = [70, 82, 100];

/// The colours of the terrain in every view, in linear RGBA.
#[derive(Debug, Clone, PartialEq)]
pub struct MapPalette {
    biomes: Vec<[f32; 4]>,
    unknown_biome: [f32; 4],
    water: [f32; 4],
    /// For each field in [`Field::ALL`] order, its ramp.
    fields: Vec<Box<[[f32; 4]]>>,
}

impl MapPalette {
    /// The palette for terrain whose biomes come from `table`.
    #[must_use]
    pub fn new(table: &BiomeTable) -> Self {
        // A table has fewer than 2^16 biomes, so each has an index.
        let biomes = (0..=u16::MAX)
            .take(table.biome_count())
            .map(|index| linear(table.biome(BiomeIndex(index)).map_colour()))
            .collect();
        let fields = Field::ALL
            .iter()
            .map(|&field| {
                (0..=u16::MAX)
                    .step_by(1 << FIELD_STEP_BITS)
                    .map(|value| linear(field_colour(field, value)))
                    .collect()
            })
            .collect();
        Self {
            biomes,
            unknown_biome: linear(UNKNOWN_BIOME),
            water: linear(WATER),
            fields,
        }
    }

    /// The colour of `tile` in `view`.
    pub(crate) fn colour(&self, view: TerrainView, tile: &Tile) -> [f32; 4] {
        match view {
            TerrainView::Biomes => self
                .biomes
                .get(usize::from(tile.biome.0))
                .copied()
                .unwrap_or(self.unknown_biome),
            TerrainView::Field(Field::Moisture | Field::Vegetation)
                if tile.fields.elevation < SEA_LEVEL =>
            {
                self.water
            }
            TerrainView::Field(field) => {
                let position = Field::ALL
                    .iter()
                    .position(|&each| each == field)
                    .expect("every field is in `Field::ALL`");
                let step = usize::from(tile.fields.get(field) >> FIELD_STEP_BITS);
                self.fields[position][step]
            }
        }
    }
}

/// The sRGB colour of `value` of `field`, on that field's ramp.
fn field_colour(field: Field, value: u16) -> [u8; 3] {
    let unit = field.to_unit(value);
    match field {
        // Water and land on separate ramps, so the coast is a sharp line.
        Field::Elevation if value < SEA_LEVEL => {
            ramp(&[(-1.0, [10, 26, 70]), (0.0, [110, 170, 220])], unit)
        }
        Field::Elevation => ramp(
            &[
                (0.0, [60, 120, 60]),
                (0.4, [190, 170, 100]),
                (0.8, [130, 100, 80]),
                (1.0, [250, 250, 250]),
            ],
            unit,
        ),
        // Diverging around freezing, over the range where climates differ.
        Field::WinterTemperature | Field::SummerTemperature => ramp(
            &[
                (-40.0, [40, 30, 120]),
                (-20.0, [60, 120, 220]),
                (0.0, [240, 240, 240]),
                (15.0, [240, 200, 90]),
                (30.0, [220, 80, 40]),
                (45.0, [110, 10, 30]),
            ],
            unit,
        ),
        Field::Moisture => ramp(
            &[
                (0.0, [190, 150, 90]),
                (0.5, [150, 190, 150]),
                (1.0, [20, 70, 170]),
            ],
            unit,
        ),
        Field::Vegetation => ramp(
            &[
                (0.0, [220, 205, 160]),
                (0.5, [130, 170, 80]),
                (1.0, [10, 70, 20]),
            ],
            unit,
        ),
    }
}

/// The colour at `value` on the ramp through `stops`, in increasing order of
/// value, interpolated in sRGB. Values beyond the ends take the end colours.
fn ramp(stops: &[(f64, [u8; 3])], value: f64) -> [u8; 3] {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if value <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let ((low, from), (high, to)) = (pair[0], pair[1]);
        if value <= high {
            let fraction = (value - low) / (high - low);
            return std::array::from_fn(|channel| {
                let (from, to) = (f64::from(from[channel]), f64::from(to[channel]));
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "rounded between two channel values"
                )]
                let mixed = (from + (to - from) * fraction).round() as u8;
                mixed
            });
        }
    }
    last.1
}

/// An opaque sRGB colour in linear RGBA, for an sRGB surface.
fn linear(srgb: [u8; 3]) -> [f32; 4] {
    let channel = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    [channel(srgb[0]), channel(srgb[1]), channel(srgb[2]), 1.0]
}
