mod error;
mod rust_imports;
mod rust_objects;

pub use error::InspectorError;

use std::path::Path;

pub struct RustAnalyzer;

impl lang_core::LanguageAnalyzer for RustAnalyzer {
    fn module_deps(
        &self,
        path: &Path,
    ) -> Result<Vec<lang_core::ModuleDep>, Box<dyn std::error::Error + Send + Sync>> {
        rust_imports::extract(path)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

impl lang_core::ObjectAnalyzer for RustAnalyzer {
    fn object_defs(
        &self,
        path: &Path,
    ) -> Result<Vec<lang_core::ClassDef>, Box<dyn std::error::Error + Send + Sync>> {
        rust_objects::extract(path)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
}

impl lang_core::ModuleNamer for RustAnalyzer {
    fn file_extension(&self) -> lang_core::FileExtension {
        lang_core::FileExtension("rs")
    }

    fn path_to_module_name(&self, rel_path: &Path) -> lang_core::ModuleName {
        let s = rel_path.to_string_lossy();
        let without_ext = s.strip_suffix(".rs").unwrap_or(&s);
        let normalized = without_ext
            .strip_suffix("/lib")
            .or_else(|| without_ext.strip_suffix("/main"))
            .or_else(|| without_ext.strip_suffix("/mod"))
            .unwrap_or(without_ext);
        lang_core::ModuleName::new(normalized.replace(['/', '\\'], "."))
    }
}
