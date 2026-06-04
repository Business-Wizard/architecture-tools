mod error;
mod model;
mod pyreverse;
mod runner;

pub use error::InspectorError;
pub use model::{InspectResult, ModuleDep};

use std::path::Path;
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_mins(2);

/// Run pyreverse against `package_path` and return module dependency edges.
///
/// # Errors
/// Returns `InspectorError` if pyreverse fails or produces unparseable output.
pub async fn inspect(package_path: &Path) -> Result<InspectResult, InspectorError> {
    inspect_with_timeout(package_path, DEFAULT_TIMEOUT).await
}

/// # Errors
/// Returns `InspectorError` if pyreverse fails or produces unparseable output.
pub async fn inspect_with_timeout(
    package_path: &Path,
    timeout: Duration,
) -> Result<InspectResult, InspectorError> {
    let module_deps = pyreverse::extract_module_deps(package_path, timeout).await?;
    Ok(InspectResult { module_deps })
}
