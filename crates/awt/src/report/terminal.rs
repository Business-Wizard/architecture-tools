use crate::graph::violations::{GraphSeverity, GraphViolation, ViolationKind};

pub fn print_graph_violations_section(violations: &[GraphViolation], root: &camino::Utf8Path) {
    if violations.is_empty() {
        println!("\n─── Graph Analysis: no violations ───────────────────────");
        return;
    }

    let errors = violations
        .iter()
        .filter(|v| v.severity == GraphSeverity::Error)
        .count();
    let warnings = violations
        .iter()
        .filter(|v| v.severity == GraphSeverity::Warning)
        .count();

    println!("\n─── Graph Violations ────────────────────────────────────");
    println!("  {errors} error(s)  {warnings} warning(s)");
    println!();

    for v in violations {
        match &v.kind {
            ViolationKind::CyclicDependency { modules } => {
                println!("  CYCLE        {}", v.message);
                for m in modules {
                    if let Some(path) = module_to_file_path(m, root) {
                        println!("               {path}");
                    }
                }
            }
            ViolationKind::ModuleHub {
                module,
                fan_in,
                threshold,
            } => {
                println!("  HUB          {}", v.message);
                if let Some(path) = module_to_file_path(module, root) {
                    println!("               {path}  (fan-in: {fan_in}, threshold: {threshold})");
                }
            }
            ViolationKind::GodModule {
                module,
                fan_out,
                threshold,
            } => {
                println!("  GOD-MODULE   {}", v.message);
                if let Some(path) = module_to_file_path(module, root) {
                    println!("               {path}  (fan-out: {fan_out}, threshold: {threshold})");
                }
            }
        }
    }
}

fn module_to_file_path(module: &str, root: &camino::Utf8Path) -> Option<camino::Utf8PathBuf> {
    if module.is_empty() {
        return None;
    }
    let rel = format!("{}.py", module.replace('.', "/"));
    let candidate = root.join(&rel);
    if !candidate.exists() {
        return None;
    }
    let cwd = std::env::current_dir().ok()?;
    let cwd = camino::Utf8Path::from_path(&cwd)?;
    candidate
        .strip_prefix(cwd)
        .ok()
        .map(camino::Utf8Path::to_owned)
        .or(Some(candidate))
}
