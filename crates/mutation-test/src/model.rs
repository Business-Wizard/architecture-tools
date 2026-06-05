use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Stable mutant identity ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MutantId(pub String);

impl MutantId {
    #[must_use]
    pub fn new(file: &str, symbol: &str, operator: &str) -> Self {
        Self(format!("{file}::{symbol}::{operator}"))
    }
}

impl std::fmt::Display for MutantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ── Candidate kinds ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateKind {
    Function,
    Method,
    Constructor,
    Import,
    Module,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperatorKind {
    AddRequiredParameter,
    RenameParameter,
    RemoveParameter,
    RemoveImport,
    RemoveModule,
    MoveModule,
}

impl std::fmt::Display for OperatorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::AddRequiredParameter => "add_required_parameter",
            Self::RenameParameter => "rename_parameter",
            Self::RemoveParameter => "remove_parameter",
            Self::RemoveImport => "remove_import",
            Self::RemoveModule => "remove_module",
            Self::MoveModule => "move_module",
        };
        f.write_str(s)
    }
}

impl OperatorKind {
    #[must_use]
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::AddRequiredParameter => "+param",
            Self::RenameParameter => "~param",
            Self::RemoveParameter => "-param",
            Self::RemoveImport => "-import",
            Self::RemoveModule => "-module",
            Self::MoveModule => "mv",
        }
    }
}

// ── Mutation candidate ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: MutantId,
    pub file: Utf8PathBuf,
    pub symbol: String,
    pub kind: CandidateKind,
    pub operator: OperatorKind,
    pub line: u32,
    pub byte_start: usize,
    pub byte_end: usize,
}

// ── Mutation error ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum MutationError {
    #[error("candidate byte range out of bounds")]
    OutOfBounds,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutant_id_new_should_format_with_double_colons() {
        let actual = MutantId::new("src/foo.py", "my_func", "add_required_parameter");
        let expected = MutantId("src/foo.py::my_func::add_required_parameter".into());
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_mutant_id_display_should_match_inner_string() {
        let id = MutantId::new("src/foo.py", "my_func", "add_required_parameter");
        let actual = id.to_string();
        let expected = "src/foo.py::my_func::add_required_parameter";
        assert_eq!(actual, expected);
    }
}
