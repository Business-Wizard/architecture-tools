use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Stable mutant identity ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MutantId(pub String);

impl MutantId {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// ── Failure evidence ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifierKind {
    Basedpyright,
    Pytest,
}

impl std::fmt::Display for VerifierKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Basedpyright => "basedpyright",
            Self::Pytest => "pytest",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureCategory {
    Syntax,
    Type,
    Import,
    TestAssertion,
    TestCollection,
    Runtime,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureScope {
    Local,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureEvent {
    pub mutant_id: MutantId,
    pub command: VerifierKind,
    pub file: Utf8PathBuf,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub symbol: Option<String>,
    pub category: FailureCategory,
    pub message: String,
    pub scope: FailureScope,
}

// ── Mutant result ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutantStatus {
    Breaks,
    Survives,
    Timeout,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutantResult {
    pub candidate: Candidate,
    pub status: MutantStatus,
    pub local_failures: Vec<FailureEvent>,
    pub external_failures: Vec<FailureEvent>,
}

impl MutantResult {
    pub fn affected_files(&self) -> Vec<&Utf8PathBuf> {
        let mut files: Vec<&Utf8PathBuf> = self.external_failures.iter().map(|f| &f.file).collect();
        files.sort();
        files.dedup();
        files
    }
}

// ── Baseline ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerifierStatus {
    Pass,
    Fail(Vec<String>),
}

impl VerifierStatus {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResult {
    pub basedpyright: VerifierStatus,
    pub pytest: VerifierStatus,
}

impl BaselineResult {
    pub fn all_pass(&self) -> bool {
        self.basedpyright.is_pass() && self.pytest.is_pass()
    }
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("repo path not found: {0}")]
    RepoNotFound(Utf8PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum MutationError {
    #[error("candidate byte range out of bounds")]
    OutOfBounds,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("command timed out after {0}s")]
    Timeout(u64),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("temp dir error: {0}")]
    TempDir(String),
}
