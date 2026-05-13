use std::path::Path;

use ignore::WalkBuilder;

use crate::config::Config;
use crate::model::{Candidate, CandidateKind, DiscoveryError, MutantId, OperatorKind};
use crate::python_ast::{self, ParsedFile};
use crate::repo;

#[derive(Debug, Default)]
pub struct CandidateCounts {
    pub functions: usize,
    pub methods: usize,
    pub constructors: usize,
    pub imports: usize,
    pub modules: usize,
}

pub struct DiscoveryResult {
    pub candidates: Vec<Candidate>,
    pub counts: CandidateCounts,
}

pub fn discover(repo_root: &Path, cfg: &Config) -> Result<DiscoveryResult, DiscoveryError> {
    let mut candidates = Vec::new();
    let mut counts = CandidateCounts::default();

    let exclude_dirs = cfg.exclude_dirs.clone();
    let walker = WalkBuilder::new(repo_root)
        .hidden(false)
        .filter_entry(move |e| {
            let name = e.file_name().to_string_lossy();
            !exclude_dirs.iter().any(|ex| name == ex.as_str())
        })
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("py") {
            continue;
        }

        let rel = match path.strip_prefix(repo_root) {
            Ok(r) => repo::to_utf8(r),
            Err(_) => continue,
        };

        let source = match std::fs::read(path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let parsed = match ParsedFile::parse(&source) {
            Some(p) => p,
            None => continue,
        };

        if cfg.operators.add_required_parameter {
            for func in python_ast::find_functions(&parsed) {
                if !func.is_eligible_for_add_param() {
                    continue;
                }

                let kind = if func.is_constructor {
                    counts.constructors += 1;
                    CandidateKind::Constructor
                } else if func.is_method {
                    counts.methods += 1;
                    CandidateKind::Method
                } else {
                    counts.functions += 1;
                    CandidateKind::Function
                };

                let id = MutantId::new(
                    rel.as_str(),
                    &func.name,
                    &OperatorKind::AddRequiredParameter.to_string(),
                );

                candidates.push(Candidate {
                    id,
                    file: rel.clone(),
                    symbol: func.name,
                    kind,
                    operator: OperatorKind::AddRequiredParameter,
                    line: func.line,
                    byte_start: func.params_byte_start,
                    byte_end: func.params_byte_end,
                });
            }
        }

        if cfg.operators.remove_import {
            for imp in python_ast::find_imports(&parsed) {
                counts.imports += 1;
                let id = MutantId::new(
                    rel.as_str(),
                    &imp.module_path,
                    &OperatorKind::RemoveImport.to_string(),
                );
                candidates.push(Candidate {
                    id,
                    file: rel.clone(),
                    symbol: imp.module_path,
                    kind: CandidateKind::Import,
                    operator: OperatorKind::RemoveImport,
                    line: imp.line,
                    byte_start: imp.byte_start,
                    byte_end: imp.byte_end,
                });
            }
        }

        if cfg.operators.remove_module || cfg.operators.move_module {
            counts.modules += 1;
        }
    }

    Ok(DiscoveryResult { candidates, counts })
}
