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
    "node_modules",
];

const ROOT_CONFIG_FILES: &[&str] = &[
    "pyproject.toml",
    "uv.lock",
    "uv.toml",
    "pyrightconfig.json",
    "pytest.ini",
    "setup.cfg",
    ".python-version",
    "conftest.py",
];

pub struct TempRepo {
    dir: TempDir,
}

impl TempRepo {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn copy_from(repo_root: &Path, include_dirs: &[String]) -> Result<Self, RunnerError> {
        let dir = tempfile::Builder::new()
            .prefix("awt-mutant-")
            .tempdir()
            .map_err(|e| RunnerError::TempDir(e.to_string()))?;
        copy_whitelisted(repo_root, dir.path(), include_dirs)?;
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

fn copy_whitelisted(src: &Path, dest: &Path, include_dirs: &[String]) -> Result<(), RunnerError> {
    for name in ROOT_CONFIG_FILES {
        let from = src.join(name);
        if from.exists() {
            link_or_copy(&from, &dest.join(name))?;
        }
    }

    for dir in include_dirs {
        copy_dir_tree(src, dest, &src.join(dir))?;
    }

    for test_dir in find_test_dirs(src) {
        copy_dir_tree(src, dest, &test_dir)?;
    }

    Ok(())
}

fn find_test_dirs(repo_root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(repo_root) else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if !p.is_dir() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let is_test = matches!(name.as_str(), "test" | "tests")
                || name.starts_with("test_")
                || name.ends_with("_test");
            is_test.then_some(p)
        })
        .collect()
}

fn copy_dir_tree(src_root: &Path, dest_root: &Path, dir: &Path) -> Result<(), RunnerError> {
    if !dir.exists() {
        return Ok(());
    }
    let walker = WalkBuilder::new(dir)
        .hidden(false)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !EXCLUDE_DIRS.iter().any(|ex| name == *ex)
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(src_root)
            .map_err(|_| RunnerError::TempDir("strip_prefix failed".into()))?;
        let target = dest_root.join(rel);

        if path.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            link_or_copy(path, &target)?;
        }
    }
    Ok(())
}

fn link_or_copy(from: &Path, to: &Path) -> Result<(), RunnerError> {
    if std::fs::hard_link(from, to).is_err() {
        std::fs::copy(from, to)?;
    }
    Ok(())
}
