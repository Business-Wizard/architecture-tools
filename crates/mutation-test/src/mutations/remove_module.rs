use std::path::Path;

use crate::model::MutationError;

pub fn apply(repo_root: &Path, rel_path: &str) -> Result<(), MutationError> {
    let target = repo_root.join(rel_path);
    std::fs::remove_file(&target)?;
    Ok(())
}
