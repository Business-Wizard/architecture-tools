use architecture_core::analyzer::{
    AbstractnessStrategy, CompositionalAbstractness, DependencyGraphInstability,
    InstabilityStrategy,
};
use architecture_core::model::ArchitectureGraph;
use architecture_core::policy::{
    AnalysisPolicy, CyclePolicy, DependencyGranularity, ExternalTypePolicy, UnknownTypePolicy,
};
use comfy_table::{Cell, Color, Table};

pub fn print_architecture_report(graph: &ArchitectureGraph) {
    if graph.modules.is_empty() {
        return;
    }

    let policy = AnalysisPolicy {
        external_type_policy: ExternalTypePolicy::Ignore,
        unknown_type_policy: UnknownTypePolicy::Ignore,
        module_dependency_granularity: DependencyGranularity::UniqueTarget,
        object_dependency_granularity: DependencyGranularity::UniqueTarget,
        cycle_policy: CyclePolicy::BreakWithZero,
    };

    let instability = DependencyGraphInstability;
    let abstractness = CompositionalAbstractness::new();

    println!("\n─── Architecture Core: Module Metrics ───────────────────────────────");

    let mut table = Table::new();
    table.set_header(["Module", "Objects", "Inst", "Abst", "Dist", "Status"]);

    let mut rows: Vec<(String, usize, f64, f64, f64)> = graph
        .modules
        .values()
        .filter_map(|module| {
            let i = instability
                .module_instability(graph, module.id, &policy)
                .ok()?
                .ratio
                .score
                .map_or(0.0, |s| s.value);
            let a = abstractness
                .module_abstractness(graph, module.id, &policy)
                .ok()?
                .ratio
                .score
                .map_or(0.0, |s| s.value);
            let dist = (a + i - 1.0_f64).abs();
            let objects = module.object_ids.len();
            Some((module.name.0.clone(), objects, i, a, dist))
        })
        .collect();

    rows.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, objects, inst, abst, dist) in rows {
        let (status, color) = status_cell(dist);
        table.add_row([
            Cell::new(&name),
            Cell::new(objects),
            Cell::new(format!("{inst:.2}")),
            Cell::new(format!("{abst:.2}")),
            Cell::new(format!("{dist:.2}")),
            Cell::new(status).fg(color),
        ]);
    }

    println!("{table}");
}

fn status_cell(dist: f64) -> (&'static str, Color) {
    if dist > 0.5 {
        ("FAIL", Color::Red)
    } else if dist > 0.3 {
        ("warn", Color::Yellow)
    } else {
        ("ok", Color::Green)
    }
}
