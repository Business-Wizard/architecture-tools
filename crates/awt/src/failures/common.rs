use std::path::Path;

use camino::Utf8PathBuf;

pub fn relativize(filename: &str, repo_root: &Path) -> Utf8PathBuf {
    let p = std::path::Path::new(filename);
    if let Ok(rel) = p.strip_prefix(repo_root) {
        return Utf8PathBuf::try_from(rel.to_path_buf())
            .unwrap_or_else(|_| Utf8PathBuf::from(filename));
    }
    // Retry after resolving symlinks (e.g. macOS /tmp → /private/tmp)
    if let (Ok(canon_p), Ok(canon_root)) = (p.canonicalize(), repo_root.canonicalize())
        && let Ok(rel) = canon_p.strip_prefix(&canon_root)
    {
        return Utf8PathBuf::try_from(rel.to_path_buf())
            .unwrap_or_else(|_| Utf8PathBuf::from(filename));
    }
    Utf8PathBuf::from(filename)
}
