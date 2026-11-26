//! Stand-in: replaced by recipe files when M1 item 7 chooses a file format for
//! the region mapping.
//!
//! The placeholder of every logical ID that the default content names. The
//! grounds are named after the biomes that use them, and the sprites after
//! the obstacle kinds. Patterns are decoration; water and peaks use patterns
//! that no passable ground uses.

use expanse_cook::SEASONS;
use expanse_cook::placeholder::{
    Colours, GroundRecipe, Pattern, Recipes, Rgb, Shape, SpriteRecipe,
};
use expanse_core::asset::{GroundId, SpriteId};

/// Every recipe of the default content.
pub fn recipes() -> Recipes {
    Recipes {
        grounds: grounds(),
        sprites: sprites(),
    }
}

fn ground(name: &str, colours: Colours, pattern: Pattern, frames: u32) -> GroundRecipe {
    GroundRecipe {
        id: GroundId::new(name).expect("default ground names are valid"),
        colours,
        pattern,
        frames,
    }
}

fn sprite(
    name: &str,
    colours: Colours,
    shape: Shape,
    tiles: [u32; 2],
    anchor_tile: [u32; 2],
) -> SpriteRecipe {
    SpriteRecipe {
        id: SpriteId::new(name).expect("default sprite names are valid"),
        colours,
        shape,
        tiles,
        anchor_tile,
    }
}

/// The colours of something green through the year, from its summer colour.
fn seasonal(summer: Rgb) -> Colours {
    let towards = |target: Rgb, share: f32| -> Rgb {
        let mut colour = summer;
        for (channel, target) in colour.iter_mut().zip(target) {
            let mixed = f32::from(*channel) + (f32::from(target) - f32::from(*channel)) * share;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a mix of two channels stays within 0..=255"
            )]
            {
                *channel = mixed.round() as u8;
            }
        }
        colour
    };
    let spring = towards([150, 200, 90], 0.3);
    let fall = towards([196, 140, 56], 0.45);
    let winter = towards([226, 232, 238], 0.6);
    let colours: [Rgb; SEASONS] = [spring, summer, fall, winter];
    Colours::Seasonal(colours)
}

fn grounds() -> Vec<GroundRecipe> {
    use Colours::Constant;
    use Pattern::{Checks, CrossHatching, Dots, Hatching, Solid, Waves};
    vec![
        ground("deep_water", Constant([24, 52, 104]), Waves, 4),
        ground("shallow_water", Constant([46, 98, 160]), Waves, 4),
        ground("peaks", Constant([236, 236, 240]), CrossHatching, 1),
        ground("ice_sheet", Constant([208, 226, 236]), Solid, 1),
        ground("highlands", Constant([128, 120, 108]), Checks, 1),
        ground("tundra", seasonal([150, 160, 132]), Dots, 1),
        ground("taiga", seasonal([46, 86, 64]), Hatching, 1),
        ground("boreal_shrubland", seasonal([124, 138, 98]), Dots, 1),
        ground("desert", Constant([220, 196, 132]), Solid, 1),
        ground("rainforest", Constant([24, 96, 40]), Hatching, 1),
        ground("savanna", Constant([190, 176, 92]), Dots, 1),
        ground("temperate_forest", seasonal([54, 112, 52]), Hatching, 1),
        ground("steppe", seasonal([176, 172, 110]), Solid, 1),
        ground("grassland", seasonal([118, 168, 80]), Dots, 1),
    ]
}

fn sprites() -> Vec<SpriteRecipe> {
    use Colours::Constant;
    use Shape::{Boulder, Canopy, Capsule, Cone};
    vec![
        sprite("rock", Constant([132, 128, 122]), Boulder, [1, 1], [0, 0]),
        sprite(
            "boulder",
            Constant([112, 108, 102]),
            Boulder,
            [2, 2],
            [0, 0],
        ),
        sprite(
            "broadleaf_tree",
            seasonal([60, 124, 50]),
            Canopy,
            [3, 3],
            [1, 2],
        ),
        sprite("conifer", Constant([34, 78, 54]), Cone, [3, 3], [1, 2]),
        sprite("palm", Constant([74, 146, 62]), Canopy, [3, 3], [1, 2]),
        sprite(
            "fallen_log",
            Constant([124, 88, 54]),
            Capsule,
            [3, 1],
            [0, 0],
        ),
        sprite("cactus", Constant([84, 140, 84]), Capsule, [1, 2], [0, 1]),
        sprite("shrub", seasonal([82, 130, 60]), Boulder, [1, 1], [0, 0]),
        sprite(
            "wanderer",
            Constant([224, 143, 36]),
            Boulder,
            [1, 1],
            [0, 0],
        ),
    ]
}
