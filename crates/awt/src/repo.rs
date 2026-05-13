use std::path::{Path, PathBuf};

use camino::Utf8PathBuf;

use crate::model::DiscoveryError;

pub fn resolve(repo_arg: Option<&Utf8PathBuf>) -> Result<PathBuf, DiscoveryError> {
    let path = repo_arg.map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        |p| p.as_std_path().to_path_buf(),
    );

    if !path.exists() {
        let display =
            Utf8PathBuf::try_from(path.clone()).unwrap_or_else(|_| Utf8PathBuf::from("?"));
        return Err(DiscoveryError::RepoNotFound(display));
    }

    Ok(path.canonicalize().unwrap_or(path))
}

pub fn to_utf8(path: &Path) -> Utf8PathBuf {
    Utf8PathBuf::try_from(path.to_path_buf())
        .unwrap_or_else(|_| Utf8PathBuf::from(path.to_string_lossy().as_ref()))
}
