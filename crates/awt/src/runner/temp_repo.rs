use std::path::{Component, Path, PathBuf};

use camino::Utf8PathBuf;
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

/// A path that is guaranteed to be relative and free of `..` components.
/// The only constructor is [`RepoRelPath::try_from_candidate`], which rejects
/// absolute paths and directory traversal. Passing this to
/// [`TempRepo::write_mutated_file`] is the only way to write into a temp repo,
/// making source-file corruption a compile-time rather than runtime concern.
pub struct RepoRelPath(Utf8PathBuf);

impl RepoRelPath {
    pub fn try_from_candidate(path: &Utf8PathBuf) -> Result<Self, RunnerError> {
        if path.is_absolute() {
            return Err(RunnerError::TempDir(format!(
                "candidate path must be relative, got: {path}"
            )));
        }
        let has_traversal = path
            .as_std_path()
            .components()
            .any(|c| c == Component::ParentDir);
        if has_traversal {
            return Err(RunnerError::TempDir(format!(
                "candidate path must not contain '..', got: {path}"
            )));
        }
        Ok(Self(path.clone()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

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

    pub fn write_mutated_file(
        &self,
        rel_path: &RepoRelPath,
        content: &[u8],
    ) -> Result<(), RunnerError> {
        let dest = self.dir.path().join(rel_path.as_str());
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Remove before writing: dest may be a hardlink sharing an inode with the source
        // file. std::fs::write truncates in place (same inode), which would corrupt the
        // original. Removing first breaks the hardlink; the subsequent write creates a
        // fresh inode at dest only.
        let _ = std::fs::remove_file(&dest);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_repo(tmp: &Path, include_dirs: &[&str], files: &[(&str, &str)]) -> Vec<PathBuf> {
        for &dir in include_dirs {
            std::fs::create_dir_all(tmp.join(dir)).unwrap();
        }
        files
            .iter()
            .map(|(rel, content)| {
                let p = tmp.join(rel);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&p, content).unwrap();
                p
            })
            .collect()
    }

    fn rel(s: &str) -> RepoRelPath {
        RepoRelPath::try_from_candidate(&Utf8PathBuf::from(s)).unwrap()
    }

    #[test]
    fn test_write_mutated_file_should_not_modify_source_file() {
        let src_dir = tempfile::tempdir().unwrap();
        let src_file = "src/domain.py";
        make_repo(src_dir.path(), &["src"], &[(src_file, "def foo(): pass\n")]);

        let include_dirs = vec!["src".to_string()];
        let temp = TempRepo::copy_from(src_dir.path(), &include_dirs).unwrap();

        temp.write_mutated_file(
            &rel(src_file),
            b"def foo(awt_required_probe: object): pass\n",
        )
        .unwrap();

        let actual = std::fs::read_to_string(src_dir.path().join(src_file)).unwrap();
        assert_eq!(actual, "def foo(): pass\n");
    }

    #[test]
    fn test_repo_rel_path_should_reject_absolute_path() {
        let result = RepoRelPath::try_from_candidate(&Utf8PathBuf::from("/etc/passwd"));
        assert!(result.is_err());
    }

    #[test]
    fn test_repo_rel_path_should_reject_traversal() {
        let result = RepoRelPath::try_from_candidate(&Utf8PathBuf::from("../../etc/passwd"));
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_from_should_not_write_outside_temp_dir() {
        let src_dir = tempfile::tempdir().unwrap();
        let src_file = "src/service.py";
        make_repo(src_dir.path(), &["src"], &[(src_file, "original\n")]);

        let include_dirs = vec!["src".to_string()];
        let temp = TempRepo::copy_from(src_dir.path(), &include_dirs).unwrap();

        // temp dir must be distinct from src dir
        assert_ne!(temp.path(), src_dir.path());

        // source content must be unchanged after copy
        let actual = std::fs::read_to_string(src_dir.path().join(src_file)).unwrap();
        assert_eq!(actual, "original\n");
    }

    #[test]
    fn test_copy_from_should_include_source_files_in_temp() {
        let src_dir = tempfile::tempdir().unwrap();
        make_repo(
            src_dir.path(),
            &["src"],
            &[("src/model.py", "class Model: pass\n")],
        );

        let include_dirs = vec!["src".to_string()];
        let temp = TempRepo::copy_from(src_dir.path(), &include_dirs).unwrap();

        let copied = std::fs::read_to_string(temp.path().join("src/model.py")).unwrap();
        assert_eq!(copied, "class Model: pass\n");
    }

    #[test]
    fn test_copy_from_should_copy_root_config_files() {
        let src_dir = tempfile::tempdir().unwrap();
        std::fs::write(src_dir.path().join("pyproject.toml"), "[project]\n").unwrap();

        let temp = TempRepo::copy_from(src_dir.path(), &[]).unwrap();

        assert!(temp.path().join("pyproject.toml").exists());
    }

    #[test]
    fn test_copy_from_should_auto_detect_tests_dir() {
        let src_dir = tempfile::tempdir().unwrap();
        make_repo(
            src_dir.path(),
            &["tests"],
            &[("tests/test_foo.py", "def test_foo(): pass\n")],
        );

        let temp = TempRepo::copy_from(src_dir.path(), &[]).unwrap();

        assert!(temp.path().join("tests/test_foo.py").exists());
    }

    #[test]
    fn test_find_test_dirs_should_match_test_prefix_and_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        for name in &["tests", "test", "test_unit", "unit_test", "src"] {
            std::fs::create_dir(tmp.path().join(name)).unwrap();
        }

        let found: Vec<String> = find_test_dirs(tmp.path())
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert!(found.contains(&"tests".to_string()));
        assert!(found.contains(&"test".to_string()));
        assert!(found.contains(&"test_unit".to_string()));
        assert!(found.contains(&"unit_test".to_string()));
        assert!(!found.contains(&"src".to_string()));
    }
}
