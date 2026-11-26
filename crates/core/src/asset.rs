//! Logical asset IDs: how content names what something looks like, never the
//! file the art comes from.
//!
//! There are two kinds, each its own type and namespace: a ground is drawn as
//! blob tiles, a sprite as one image at an anchor. A name is in snake case and
//! unique within its kind, so a ground and a sprite may share one.

use std::error::Error;
use std::fmt;

/// The logical ID of a ground: a kind of land drawn as blob tiles.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroundId(String);

/// The logical ID of a sprite: one image, drawn at an anchor tile.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpriteId(String);

macro_rules! asset_id {
    ($id:ident) => {
        impl $id {
            /// Checks that `name` is in snake case.
            ///
            /// # Errors
            ///
            /// Returns an error if `name` is empty, starts with something
            /// other than a lowercase letter, or holds anything other than
            /// lowercase letters, digits and single underscores between them.
            pub fn new(name: impl Into<String>) -> Result<Self, InvalidAssetName> {
                let name = name.into();
                if is_snake_case(&name) {
                    Ok(Self(name))
                } else {
                    Err(InvalidAssetName(name))
                }
            }

            /// The name.
            #[must_use]
            pub fn name(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $id {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

asset_id!(GroundId);
asset_id!(SpriteId);

fn is_snake_case(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && !name.ends_with('_')
        && !name.contains("__")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A name rejected as a logical asset ID because it is not in snake case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidAssetName(pub String);

impl fmt::Display for InvalidAssetName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a valid asset name: use lowercase letters, digits and \
             single underscores, starting with a letter",
            self.0
        )
    }
}

impl Error for InvalidAssetName {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches a check that lets through names a file format or a path would
    /// mangle, or that rejects ordinary snake case.
    #[test]
    fn only_snake_case_names_are_accepted() {
        for valid in ["grass", "deep_water", "tree2", "a_1_b"] {
            assert!(GroundId::new(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "Grass",
            "deep-water",
            "_grass",
            "grass_",
            "a__b",
            "2tree",
            "dé",
        ] {
            assert!(SpriteId::new(invalid).is_err(), "{invalid}");
        }
    }
}
