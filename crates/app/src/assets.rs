//! Loading the cooked assets: the container from disk, resolved against the
//! content.
//!
//! The game always loads a container, whatever art it holds, so a build
//! without the licensed pack runs the same path as one with it.

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

use expanse_cook::{ReadError, read_container};
use expanse_render::{AssetContent, Assets, MissingAssets};
use expanse_terrain::BiomeTable;

/// Reads the container at `path` and resolves every logical ID that `table`
/// and `content` name.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or read as a container, or
/// if content names art that the container lacks.
pub fn load_assets(
    path: &Path,
    table: &BiomeTable,
    content: &AssetContent,
) -> Result<Assets, LoadAssetsError> {
    let file = File::open(path).map_err(|err| LoadAssetsError::Open {
        path: path.to_owned(),
        err,
    })?;
    let container =
        read_container(&mut BufReader::new(file)).map_err(|err| LoadAssetsError::Read {
            path: path.to_owned(),
            err,
        })?;
    Assets::resolve(container, table, content).map_err(LoadAssetsError::Missing)
}

/// Why the cooked assets could not be loaded.
#[derive(Debug)]
pub enum LoadAssetsError {
    /// The file cannot be opened.
    Open { path: PathBuf, err: io::Error },
    /// The file is not a readable container.
    Read { path: PathBuf, err: ReadError },
    /// Content names art that the container lacks.
    Missing(MissingAssets),
}

impl fmt::Display for LoadAssetsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, err } if err.kind() == io::ErrorKind::NotFound => write!(
                f,
                "no cooked assets at `{}`; cook them first (`just cook`)",
                path.display()
            ),
            Self::Open { path, err } => {
                write!(
                    f,
                    "cannot open the cooked assets at `{}`: {err}",
                    path.display()
                )
            }
            Self::Read { path, err } => write!(f, "`{}`: {err}", path.display()),
            Self::Missing(err) => err.fmt(f),
        }
    }
}

impl Error for LoadAssetsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Open { err, .. } => Some(err),
            Self::Read { err, .. } => Some(err),
            Self::Missing(err) => Some(err),
        }
    }
}
