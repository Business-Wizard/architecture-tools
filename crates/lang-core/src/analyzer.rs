use std::path::Path;

use crate::ModuleDep;

/// Interface every language backend must implement.
/// Object-safe: use `Box<dyn LanguageAnalyzer>` to dispatch at runtime.
pub trait LanguageAnalyzer {
    fn module_deps(
        &self,
        path: &Path,
    ) -> Result<Vec<ModuleDep>, Box<dyn std::error::Error + Send + Sync>>;
}
