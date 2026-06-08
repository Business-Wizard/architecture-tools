use std::path::Path;

use crate::ModuleDep;

/// Interface every language backend must implement.
/// Object-safe: use `Box<dyn LanguageAnalyzer>` to dispatch at runtime.
pub trait LanguageAnalyzer {
    /// Returns the module-level dependency edges found in the given package path.
    ///
    /// # Errors
    ///
    /// Returns an error if the path cannot be read or source files cannot be parsed.
    fn module_deps(
        &self,
        path: &Path,
    ) -> Result<Vec<ModuleDep>, Box<dyn std::error::Error + Send + Sync>>;
}
