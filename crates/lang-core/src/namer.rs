use std::path::Path;

use crate::{FileExtension, ModuleName};

/// Encapsulates language-specific file-path → dotted-module-name mapping.
/// Implement alongside `LanguageAnalyzer` on each concrete analyzer struct.
/// Object-safe: use `&dyn ModuleNamer` for infrastructure dispatch.
pub trait ModuleNamer {
    /// Source file extension for this language.
    fn file_extension(&self) -> FileExtension;

    /// Convert a repo-relative file path to a dotted module name.
    fn path_to_module_name(&self, rel_path: &Path) -> ModuleName;
}
