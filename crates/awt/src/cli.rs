use std::sync::{Arc, Mutex};
use std::time::Instant;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::config;
use crate::config::MainSequenceConfig;
use crate::discovery;
use crate::failures::{basedpyright, pytest};
use crate::fitness;
use crate::graph::coupling_graph::GraphIndex;
use crate::graph::{abstractness, graph_analysis, metrics};
use crate::model::OperatorKind;
use crate::model::{
    BaselineResult, Candidate, FailureCategory, FailureEvent, FailureScope, MutantResult,
    MutantStatus, VerifierKind, VerifierStatus,
};
use crate::mutations::{
    add_parameter, move_module, remove_import, remove_module, remove_parameter, rename_parameter,
};
use crate::repo;
use crate::report::chart;
use crate::report::dot;
use crate::report::sdp_flow;
use crate::report::summary::{self, RunReport};
use crate::report::terminal;
use crate::runner::temp_repo::{RepoRelPath, TempRepo};
use crate::runner::verifier::VerifierSet;

#[derive(Parser)]
#[command(
    name = "awt",
    about = "Architecture Wind Tunnel — mutation-based coupling analysis"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Run(RunArgs),
    Inspect(InspectArgs),
}

#[derive(Parser, Debug)]
pub struct InspectArgs {
    #[arg(help = "Path to the Python package to inspect")]
    pub path: Utf8PathBuf,

    #[arg(long, default_value_t = 120, help = "Timeout in seconds for each tool")]
    pub timeout_secs: u64,

    #[arg(
        long,
        help = "Write DOT output to awt-inspect.dot in the current directory"
    )]
    pub save: bool,
}

#[derive(Parser, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct RunArgs {
    #[arg(long, help = "Path to the Python repository to analyse")]
    pub repo: Option<Utf8PathBuf>,

    #[arg(long, help = "Path to awt.toml config file")]
    pub config: Option<Utf8PathBuf>,

    #[arg(long, help = "Maximum number of mutants to run")]
    pub max_mutants: Option<usize>,

    #[arg(long, help = "Max concurrent mutation jobs (default: CPU count - 1)")]
    pub jobs: Option<usize>,

    #[arg(long, help = "Keep temp directory when a mutation run fails")]
    pub keep_temp_on_failure: bool,

    #[arg(long, help = "Write full run results to this JSON file")]
    pub json_out: Option<Utf8PathBuf>,

    #[arg(long, help = "Compare against a previous run JSON for delta report")]
    pub compare: Option<Utf8PathBuf>,

    #[arg(
        long,
        default_value = "coupling.dot",
        help = "Write coupling graph to this .dot file"
    )]
    pub dot_out: Utf8PathBuf,

    #[arg(
        long,
        default_value = "main_sequence.png",
        help = "Write I-vs-A scatter chart to this PNG file"
    )]
    pub chart_out: Utf8PathBuf,

    #[arg(
        long,
        default_value = "sdp_flow.png",
        help = "Write SDP dependency-flow chart to this PNG file"
    )]
    pub sdp_out: Utf8PathBuf,

    #[arg(
        long,
        help = "Discover candidates and print counts without running mutations"
    )]
    pub dry_run: bool,

    #[arg(long, help = "Exit non-zero if any fitness violations are found")]
    pub fail_on_violations: bool,

    #[arg(
        long,
        help = "Enable mutation testing (slower; builds coupling graph from live mutations)"
    )]
    pub mutate: bool,
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(ref args) => run_command(args),
        Commands::Inspect(ref args) => run_inspect_command(args),
    }
}

fn run_inspect_command(args: &InspectArgs) {
    let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
    let timeout = std::time::Duration::from_secs(args.timeout_secs);
    let result = rt.block_on(py_analyzer::inspect_with_timeout(
        args.path.as_std_path(),
        timeout,
    ));
    match result {
        Ok(inspect) => {
            let dot = inspect_to_dot(&inspect);
            if args.save {
                let path = std::path::Path::new("awt-inspect.dot");
                if let Err(e) = std::fs::write(path, &dot) {
                    eprintln!("error writing awt-inspect.dot: {e}");
                    std::process::exit(1);
                }
                eprintln!("wrote awt-inspect.dot");
            } else {
                print!("{dot}");
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn inspect_to_dot(result: &py_analyzer::InspectResult) -> String {
    use std::collections::{HashMap, HashSet};
    use std::fmt::Write as _;

    let mut out = String::new();
    writeln!(out, "digraph inspection {{").unwrap();
    writeln!(out, "    rankdir=LR;").unwrap();

    if !result.module_deps.is_empty() {
        // Build a set of root prefixes that originate from the scanned package.
        // Any `to` whose first component doesn't appear here is stdlib/3rd-party noise.
        let internal_roots: HashSet<&str> = result
            .module_deps
            .iter()
            .map(|d| d.from.split('.').next().unwrap_or(d.from.as_str()))
            .collect();
        let is_internal = |name: &str| {
            let root = name.split('.').next().unwrap_or(name);
            internal_roots.contains(root)
        };

        writeln!(out).unwrap();
        let mut seen: HashSet<(&str, &str)> = HashSet::new();
        for dep in &result.module_deps {
            if is_internal(dep.to.as_str()) && seen.insert((dep.from.as_str(), dep.to.as_str())) {
                writeln!(out, "    \"{}\" -> \"{}\";", dep.from, dep.to).unwrap();
            }
        }
    }

    // Maps both bare name and qualified "module.Name" → the DOT node ID.
    // Bare-name entries are last-one-wins (ambiguous case); qualified entries are exact.
    // class_deps from py-analyzer emit qualified strings when import info is available,
    // so those lookups always resolve correctly regardless of name collisions.
    let mut class_node_id: HashMap<String, String> = HashMap::new();
    for c in &result.classes {
        let node_id = format!("{}.{}", c.module, c.name);
        class_node_id.insert(c.name.clone(), node_id.clone());
        class_node_id.insert(node_id.clone(), node_id);
    }

    if !result.classes.is_empty() {
        let mut module_order: Vec<&str> = Vec::new();
        let mut seen_modules: HashSet<&str> = HashSet::new();
        for cls in &result.classes {
            if seen_modules.insert(cls.module.as_str()) {
                module_order.push(cls.module.as_str());
            }
        }
        for module in module_order {
            let cluster_id = module.replace('.', "_");
            writeln!(out).unwrap();
            writeln!(out, "    subgraph cluster_{cluster_id} {{").unwrap();
            writeln!(out, "        label=\"{module}\";").unwrap();
            for cls in result.classes.iter().filter(|c| c.module == module) {
                let node_id = format!("{}.{}", cls.module, cls.name);
                writeln!(
                    out,
                    "        \"{}\" [shape=record, label=\"{}\"];",
                    node_id,
                    build_record_label(cls)
                )
                .unwrap();
            }
            writeln!(out, "    }}").unwrap();
        }
    }

    for cls in &result.classes {
        let src_id = format!("{}.{}", cls.module, cls.name);
        for base in &cls.bases {
            let base_name = base.split('.').next_back().unwrap_or(base.as_str());
            if let Some(base_id) = class_node_id.get(base_name) {
                writeln!(
                    out,
                    "    \"{}\" -> \"{}\" [style=dashed, label=\"extends\"];",
                    src_id, base_id
                )
                .unwrap();
            }
        }
        for dep in &cls.class_deps {
            let is_base = cls
                .bases
                .iter()
                .any(|b| b.split('.').next_back().unwrap_or(b.as_str()) == dep.as_str());
            if !is_base {
                if let Some(dep_id) = class_node_id.get(dep.as_str()) {
                    writeln!(
                        out,
                        "    \"{}\" -> \"{}\" [label=\"uses\"];",
                        src_id, dep_id
                    )
                    .unwrap();
                }
            }
        }
    }

    writeln!(out, "}}").unwrap();
    out
}

fn build_record_label(cls: &py_analyzer::ClassDef) -> String {
    let escape = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('{', "\\{")
            .replace('}', "\\}")
            .replace('|', "\\|")
            .replace('<', "\\<")
            .replace('>', "\\>")
    };

    let mut sections: Vec<String> = vec![escape(&cls.name)];

    if !cls.attributes.is_empty() {
        sections.push(
            cls.attributes
                .iter()
                .map(|a| escape(a))
                .collect::<Vec<_>>()
                .join("\\n"),
        );
    }
    if !cls.methods.is_empty() {
        sections.push(
            cls.methods
                .iter()
                .map(|m| format!("{}()", escape(m)))
                .collect::<Vec<_>>()
                .join("\\n"),
        );
    }

    format!("{{{}}}", sections.join("|"))
}

fn run_static_command(args: &RunArgs) {
    let repo_root = match repo::resolve(args.repo.as_ref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let cfg = match config::load(args.config.as_ref(), &repo_root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error loading config: {e}");
            std::process::exit(1);
        }
    };

    let discovery = discovery::discover(&repo_root, &cfg);

    if args.dry_run {
        let c = &discovery.counts;
        println!("\nCandidates discovered:");
        println!("  functions:     {}", c.functions);
        println!("  methods:       {}", c.methods);
        println!("  constructors:  {}", c.constructors);
        println!("  imports:       {}", c.imports);
        println!("  modules:       {}", c.modules);
        return;
    }

    let source_files: Vec<Utf8PathBuf> = {
        let mut seen = std::collections::HashSet::new();
        discovery
            .candidates
            .iter()
            .map(|c| c.file.clone())
            .filter(|f| seen.insert(f.clone()))
            .collect()
    };

    println!(
        "\nBuilding coupling graph from static import analysis ({} files)...",
        source_files.len()
    );

    let graph_idx = GraphIndex::build_from_source_imports(&source_files, &repo_root);
    let include_dirs: Vec<Utf8PathBuf> = cfg
        .include_dirs
        .iter()
        .map(|s| Utf8PathBuf::from(s.as_str()))
        .collect();
    let abstractness_map = abstractness::compute(&repo_root, &include_dirs);
    let metrics_result = metrics::compute(&graph_idx, &abstractness_map);
    let empty_results: &[MutantResult] = &[];
    let cluster_result = graph_analysis::analyse(&graph_idx, empty_results, &source_files);
    let fitness_report = fitness::evaluate_all(&graph_idx, &metrics_result, &cfg.fitness);

    let baseline = BaselineResult {
        basedpyright: VerifierStatus::Pass,
        pytest: VerifierStatus::Pass,
    };

    println!("Coupling graph built from static import analysis — no mutation data.\n");

    terminal::print_report(
        &baseline,
        empty_results,
        &cluster_result,
        &metrics_result,
        &fitness_report,
    );

    let arch_graph =
        crate::graph::architecture_builder::build_architecture_graph(&source_files, &repo_root);
    crate::report::architecture_report::print_architecture_report(&arch_graph);

    let report = RunReport::build(&baseline, empty_results, &cluster_result);

    write_outputs(
        args,
        &graph_idx,
        &report,
        &metrics_result,
        &cfg.fitness.main_sequence,
    );

    if args.fail_on_violations && fitness_report.has_errors() {
        std::process::exit(2);
    }
}

#[allow(clippy::too_many_lines)]
fn run_command(args: &RunArgs) {
    if !args.mutate {
        return run_static_command(args);
    }
    let repo_root = match repo::resolve(args.repo.as_ref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let mut cfg = match config::load(args.config.as_ref(), &repo_root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error loading config: {e}");
            std::process::exit(1);
        }
    };

    if let Some(n) = args.max_mutants {
        cfg.max_mutants = n;
    }
    if let Some(j) = args.jobs {
        cfg.jobs = j;
    }
    if args.keep_temp_on_failure {
        cfg.keep_temp_on_failure = true;
    }

    let verifiers = VerifierSet::new(cfg.timeout_secs, cfg.include_dirs.clone());

    let rt = Runtime::new().expect("failed to build tokio runtime");

    let baseline = rt.block_on(run_baseline_async(&verifiers, &repo_root));

    if !baseline.all_pass() {
        eprintln!("Baseline:");
        print_baseline(&baseline);
        eprintln!("\nBaseline failed. Aborting.");
        std::process::exit(1);
    }

    let discovery = discovery::discover(&repo_root, &cfg);

    if args.dry_run {
        let c = &discovery.counts;
        println!("\nCandidates discovered:");
        println!("  functions:     {}", c.functions);
        println!("  methods:       {}", c.methods);
        println!("  constructors:  {}", c.constructors);
        println!("  imports:       {}", c.imports);
        println!("  modules:       {}", c.modules);
        return;
    }

    let source_files: Vec<Utf8PathBuf> = {
        let mut seen = std::collections::HashSet::new();
        discovery
            .candidates
            .iter()
            .map(|c| c.file.clone())
            .filter(|f| seen.insert(f.clone()))
            .collect()
    };
    let candidates: Vec<Candidate> = discovery
        .candidates
        .into_iter()
        .take(cfg.max_mutants)
        .collect();

    let pb = Arc::new(ProgressBar::new(candidates.len() as u64));
    pb.set_style(
        ProgressStyle::with_template("[{pos}/{len}] {percent}%  {msg}")
            .expect("valid progress template"),
    );
    pb.println(format!(
        "\nRunning {} mutants ({} jobs)...",
        candidates.len(),
        cfg.jobs
    ));

    let eta = Arc::new(Mutex::new(EtaTracker::new(
        pb.length().unwrap_or(0),
        cfg.jobs,
    )));
    let run_cfg = MutantRunConfig {
        timeout_secs: cfg.timeout_secs,
        keep_on_fail: cfg.keep_temp_on_failure,
        jobs: cfg.jobs,
        include_dirs: cfg.include_dirs.clone(),
        pb: Arc::clone(&pb),
        eta,
    };
    let results = rt.block_on(run_mutants_async(candidates, &repo_root, run_cfg));

    pb.finish_and_clear();

    let graph_idx = GraphIndex::build(&results, &source_files);
    let include_dirs: Vec<Utf8PathBuf> = cfg
        .include_dirs
        .iter()
        .map(|s| Utf8PathBuf::from(s.as_str()))
        .collect();
    let abstractness_map = abstractness::compute(&repo_root, &include_dirs);
    let metrics_result = metrics::compute(&graph_idx, &abstractness_map);
    let cluster_result = graph_analysis::analyse(&graph_idx, &results, &source_files);
    let fitness_report = fitness::evaluate_all(&graph_idx, &metrics_result, &cfg.fitness);
    terminal::print_report(
        &baseline,
        &results,
        &cluster_result,
        &metrics_result,
        &fitness_report,
    );

    let report = RunReport::build(&baseline, &results, &cluster_result);

    write_outputs(
        args,
        &graph_idx,
        &report,
        &metrics_result,
        &cfg.fitness.main_sequence,
    );

    if args.fail_on_violations && fitness_report.has_errors() {
        std::process::exit(2);
    }
}

struct MutantRunConfig {
    timeout_secs: u64,
    keep_on_fail: bool,
    jobs: usize,
    include_dirs: Vec<String>,
    pb: Arc<ProgressBar>,
    eta: Arc<Mutex<EtaTracker>>,
}

struct EtaTracker {
    samples: Vec<f64>,
    total: u64,
    jobs: usize,
}

impl EtaTracker {
    fn new(total: u64, jobs: usize) -> Self {
        Self {
            samples: Vec::new(),
            total,
            jobs,
        }
    }

    fn record(&mut self, secs: f64) {
        self.samples.push(secs);
    }

    fn eta_string(&self) -> String {
        let n = self.samples.len();
        let completed = n as u64;
        let remaining = self.total.saturating_sub(completed);

        if remaining == 0 {
            return String::new();
        }

        if n < 2 {
            return "ETA ~...".to_string();
        }

        #[allow(clippy::cast_precision_loss)]
        let n_f = n as f64;
        #[allow(clippy::cast_precision_loss)]
        let remaining_f = remaining as f64;

        let mean = self.samples.iter().sum::<f64>() / n_f;
        // Bessel's correction (n-1): unbiased sample variance, especially important for small n
        let variance = self.samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (n_f - 1.0);
        let std_dev = variance.sqrt();

        // Divide by parallel jobs: N workers drain the queue N times faster
        #[allow(clippy::cast_precision_loss)]
        let jobs_f = self.jobs as f64;
        let eta_secs = mean * remaining_f / jobs_f;

        // 70% CI: z for 85th percentile of standard normal ≈ 1.036
        let ci_half = 1.036_f64 * std_dev * remaining_f.sqrt() / jobs_f;

        format!("ETA ~{} ±{}", fmt_duration(eta_secs), fmt_duration(ci_half))
    }
}

fn fmt_duration(secs: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let secs = secs.round() as u64;
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

async fn run_baseline_async(verifiers: &VerifierSet, repo: &std::path::Path) -> BaselineResult {
    let basedpyright = verifiers
        .run_basedpyright(repo)
        .await
        .unwrap_or_else(|e| VerifierStatus::Fail(vec![format!("runner error: {e}")]));
    let pytest = verifiers
        .run_pytest(repo)
        .await
        .unwrap_or_else(|e| VerifierStatus::Fail(vec![format!("runner error: {e}")]));
    BaselineResult {
        basedpyright,
        pytest,
    }
}

async fn run_mutants_async(
    candidates: Vec<Candidate>,
    repo_root: &std::path::Path,
    cfg: MutantRunConfig,
) -> Vec<MutantResult> {
    let sem = Arc::new(Semaphore::new(cfg.jobs));
    let include_dirs = Arc::new(cfg.include_dirs);
    let mut join_set: JoinSet<MutantResult> = JoinSet::new();

    for candidate in candidates {
        let permit = Arc::clone(&sem)
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let repo = repo_root.to_path_buf();
        let pb = Arc::clone(&cfg.pb);
        let eta = Arc::clone(&cfg.eta);
        let dirs = Arc::clone(&include_dirs);
        let timeout_secs = cfg.timeout_secs;
        let keep_on_fail = cfg.keep_on_fail;

        join_set.spawn(async move {
            let _permit = permit;
            let t0 = Instant::now();
            let result =
                run_mutant_async(candidate, &repo, timeout_secs, keep_on_fail, &dirs).await;
            let elapsed = t0.elapsed().as_secs_f64();
            let msg = {
                let mut tracker = eta.lock().expect("eta mutex poisoned");
                tracker.record(elapsed);
                tracker.eta_string()
            };
            pb.set_message(msg);
            pb.inc(1);
            result
        });
    }

    let mut results = Vec::with_capacity(join_set.len());
    while let Some(outcome) = join_set.join_next().await {
        match outcome {
            Ok(r) => results.push(r),
            Err(e) => eprintln!("mutant task panicked: {e}"),
        }
    }
    results
}

async fn run_mutant_async(
    candidate: Candidate,
    repo_root: &std::path::Path,
    timeout_secs: u64,
    keep_on_fail: bool,
    include_dirs: &[String],
) -> MutantResult {
    let candidate_clone = candidate.clone();
    let repo = repo_root.to_path_buf();
    let dirs = include_dirs.to_vec();

    let temp =
        match tokio::task::spawn_blocking(move || prepare_temp(&repo, &candidate_clone, &dirs))
            .await
            .expect("prepare_temp task panicked")
        {
            Ok(t) => t,
            Err(msg) => return invalid(&candidate, &msg),
        };

    let timeout = std::time::Duration::from_secs(timeout_secs);
    let temp_path = temp.path().to_path_buf();
    let id = &candidate.id;

    let mut all_failures: Vec<FailureEvent> = vec![];
    all_failures.extend(
        basedpyright::run_and_parse(id, &temp_path, timeout)
            .await
            .unwrap_or_default(),
    );
    all_failures.extend(
        pytest::run_and_parse(id, &temp_path, timeout)
            .await
            .unwrap_or_default(),
    );

    let any_fail = !all_failures.is_empty();
    let status = if any_fail {
        MutantStatus::Breaks
    } else {
        MutantStatus::Survives
    };

    if any_fail && keep_on_fail {
        let _ = temp.keep();
    }

    let local_failures: Vec<FailureEvent> = all_failures
        .iter()
        .filter(|f| f.scope == FailureScope::Local)
        .cloned()
        .collect();
    let external_failures: Vec<FailureEvent> = all_failures
        .into_iter()
        .filter(|f| f.scope == FailureScope::External)
        .collect();

    MutantResult {
        candidate,
        status,
        local_failures,
        external_failures,
    }
}

fn write_outputs(
    args: &RunArgs,
    graph_idx: &GraphIndex,
    report: &RunReport,
    metrics: &crate::graph::metrics::MetricsResult,
    main_sequence: &MainSequenceConfig,
) {
    if let Err(e) = dot::write_dot(graph_idx, metrics, args.dot_out.as_path()) {
        eprintln!("error writing dot output: {e}");
    }

    if let Err(e) = chart::write_chart(metrics, main_sequence, args.chart_out.as_path()) {
        eprintln!("error writing chart output: {e}");
    }

    if let Err(e) = sdp_flow::write_sdp_flow(graph_idx, metrics, args.sdp_out.as_path()) {
        eprintln!("error writing SDP flow chart: {e}");
    }

    if let Some(json_path) = &args.json_out {
        match serde_json::to_string_pretty(report) {
            Ok(json) => {
                if let Err(e) = std::fs::write(json_path.as_std_path(), json) {
                    eprintln!("error writing json output: {e}");
                }
            }
            Err(e) => eprintln!("error serialising report: {e}"),
        }
    }

    if let Some(compare_path) = &args.compare {
        match std::fs::read_to_string(compare_path.as_std_path()) {
            Ok(raw) => match serde_json::from_str::<RunReport>(&raw) {
                Ok(before) => {
                    let delta = summary::compute_delta(&before, report);
                    summary::print_delta(&delta);
                }
                Err(e) => eprintln!("error parsing compare file: {e}"),
            },
            Err(e) => eprintln!("error reading compare file: {e}"),
        }
    }
}

fn prepare_temp(
    repo_root: &std::path::Path,
    candidate: &Candidate,
    include_dirs: &[String],
) -> Result<TempRepo, String> {
    let source =
        std::fs::read(repo_root.join(candidate.file.as_str())).map_err(|e| e.to_string())?;

    let patch = match candidate.operator {
        OperatorKind::AddRequiredParameter => {
            Some(add_parameter::apply(&source, candidate).map_err(|e| e.to_string())?)
        }
        OperatorKind::RenameParameter => {
            Some(rename_parameter::apply(&source, candidate).map_err(|e| e.to_string())?)
        }
        OperatorKind::RemoveParameter => {
            Some(remove_parameter::apply(&source, candidate).map_err(|e| e.to_string())?)
        }
        OperatorKind::RemoveImport => {
            Some(remove_import::apply(&source, candidate).map_err(|e| e.to_string())?)
        }
        OperatorKind::RemoveModule | OperatorKind::MoveModule => None,
    };

    let rel = RepoRelPath::try_from_candidate(&candidate.file).map_err(|e| e.to_string())?;
    let temp = TempRepo::copy_from(repo_root, include_dirs).map_err(|e| e.to_string())?;

    match candidate.operator {
        OperatorKind::RemoveModule => {
            remove_module::apply(temp.path(), rel.as_str()).map_err(|e| e.to_string())?;
        }
        OperatorKind::MoveModule => {
            move_module::apply(temp.path(), rel.as_str()).map_err(|e| e.to_string())?;
        }
        _ => {
            temp.write_mutated_file(&rel, &patch.unwrap_or_default())
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(temp)
}

fn invalid(candidate: &Candidate, msg: &str) -> MutantResult {
    MutantResult {
        candidate: candidate.clone(),
        status: MutantStatus::Invalid,
        local_failures: vec![make_runner_failure(&candidate_stub(), msg)],
        external_failures: vec![],
    }
}

fn candidate_stub() -> camino::Utf8PathBuf {
    camino::Utf8PathBuf::from("unknown")
}

fn make_runner_failure(_file: &camino::Utf8PathBuf, msg: &str) -> FailureEvent {
    use crate::model::MutantId;
    FailureEvent {
        mutant_id: MutantId("unknown".into()),
        command: VerifierKind::Basedpyright,
        file: camino::Utf8PathBuf::from("unknown"),
        line: None,
        column: None,
        symbol: None,
        category: FailureCategory::Unknown,
        message: msg.to_string(),
        scope: FailureScope::Local,
    }
}

fn print_baseline(b: &BaselineResult) {
    print_verifier_status("basedpyright", &b.basedpyright);
    print_verifier_status("pytest", &b.pytest);
}

fn print_verifier_status(name: &str, status: &VerifierStatus) {
    match status {
        VerifierStatus::Pass => println!("  {name}: pass"),
        VerifierStatus::Fail(lines) if lines.is_empty() => {
            println!("  {name}: FAIL (no output captured — verifier may not be installed)");
        }
        VerifierStatus::Fail(lines) => {
            println!("  {name}: {} existing errors", lines.len());
            for line in lines.iter().take(5) {
                println!("    {line}");
            }
            if lines.len() > 5 {
                println!("    … ({} more)", lines.len() - 5);
            }
        }
    }
}
