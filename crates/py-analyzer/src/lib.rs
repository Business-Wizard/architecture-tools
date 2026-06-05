mod error;
mod model;
mod python_imports;

pub use error::InspectorError;
pub use model::{ClassDef, InspectResult, ModuleDep};

use std::path::Path;
use std::time::Duration;

/// Inspect a Python package directory and return module dependency edges and class definitions.
///
/// # Errors
/// Returns `InspectorError` if file I/O or parsing fails.
pub fn inspect(package_path: &Path) -> Result<InspectResult, InspectorError> {
    let (module_deps, classes) = python_imports::extract(package_path)?;
    Ok(InspectResult {
        module_deps,
        classes,
    })
}

/// Identical to [`inspect`]; the `timeout` parameter is accepted for API compatibility
/// but ignored (no subprocess is involved).
///
/// # Errors
/// Returns `InspectorError` if file I/O or parsing fails.
pub fn inspect_with_timeout(
    package_path: &Path,
    _timeout: Duration,
) -> Result<InspectResult, InspectorError> {
    inspect(package_path)
}
