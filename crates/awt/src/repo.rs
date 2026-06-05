use std::path::Path;

use camino::Utf8PathBuf;

pub fn to_utf8(path: &Path) -> Utf8PathBuf {
    Utf8PathBuf::try_from(path.to_path_buf())
        .unwrap_or_else(|_| Utf8PathBuf::from(path.to_string_lossy().as_ref()))
}
