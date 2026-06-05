use std::path::Path;

use crate::model::MutationError;

const MOVED_DIR: &str = "_awt_moved";

/// # Errors
/// Returns `MutationError` if the path has no filename, or if filesystem operations fail.
pub fn apply(repo_root: &Path, rel_path: &str) -> Result<(), MutationError> {
    let source = repo_root.join(rel_path);

    let filename = source
        .file_name()
        .ok_or(MutationError::OutOfBounds)?
        .to_os_string();

    let moved_dir = repo_root.join(MOVED_DIR);
    std::fs::create_dir_all(&moved_dir)?;

    let dest = moved_dir.join(filename);
    std::fs::rename(&source, &dest)?;

    Ok(())
}
