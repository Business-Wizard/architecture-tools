mod error;
mod model;
mod python_imports;

pub use error::InspectorError;
pub use lang_core::ModuleDep;

use std::path::Path;

pub struct PythonAnalyzer;

impl lang_core::LanguageAnalyzer for PythonAnalyzer {
    fn module_deps(
        &self,
        path: &Path,
    ) -> Result<Vec<lang_core::ModuleDep>, Box<dyn std::error::Error + Send + Sync>> {
        let (module_deps, _classes) = python_imports::extract(path)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(module_deps)
    }
}
