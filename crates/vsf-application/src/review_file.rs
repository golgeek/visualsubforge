//! Local file adapter shared by desktop and CLI. No input can select another path.
use crate::{
    Result,
    review_package::{self, Package},
};
use std::{fs::File, io::Read, path::Path};
pub fn read(path: &Path) -> Result<Package> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Cannot open review: {e}"))?;
    if !meta.is_file() {
        return Err("Choose a regular review file".into());
    }
    let f = File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    f.take(review_package::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    review_package::read(&bytes)
}
