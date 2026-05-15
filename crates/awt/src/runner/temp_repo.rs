use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use tempfile::TempDir;

use crate::model::RunnerError;

const EXCLUDE_DIRS: &[&str] = &[
    ".git",
    ".venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "node_modules",
];

pub struct TempRepo {
    dir: TempDir,
}

impl TempRepo {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn copy_from(repo_root: &Path) -> Result<Self, RunnerError> {
        let dir = tempfile::Builder::new()
            .prefix("awt-mutant-")
            .tempdir()
            .map_err(|e| RunnerError::TempDir(e.to_string()))?;
        copy_tree(repo_root, dir.path())?;
        Ok(Self { dir })
    }

    pub fn write_mutated_file(&self, rel_path: &str, content: &[u8]) -> Result<(), RunnerError> {
        let dest = self.dir.path().join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(dest, content)?;
        Ok(())
    }

    pub fn keep(self) -> PathBuf {
        self.dir.keep()
    }
}

fn copy_tree(src: &Path, dest: &Path) -> Result<(), RunnerError> {
    let walker = WalkBuilder::new(src)
        .hidden(false)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !EXCLUDE_DIRS.iter().any(|ex| name == *ex)
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(src)
            .map_err(|_| RunnerError::TempDir("strip_prefix failed".into()))?;
        let target = dest.join(rel);

        if path.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &target)?;
        }
    }

    Ok(())
}
