use std::sync::Arc;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::config;
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
use crate::report::dot;
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
}

#[derive(Parser, Debug)]
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
        help = "Discover candidates and print counts without running mutations"
    )]
    pub dry_run: bool,

    #[arg(long, help = "Exit non-zero if any fitness violations are found")]
    pub fail_on_violations: bool,
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(ref args) => run_command(args),
    }
}

fn run_command(args: &RunArgs) {
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

    let candidates: Vec<Candidate> = discovery
        .candidates
        .into_iter()
        .take(cfg.max_mutants)
        .collect();

    let pb = Arc::new(ProgressBar::new(candidates.len() as u64));
    pb.set_style(
        ProgressStyle::with_template("[{pos}/{len}] {percent}%").expect("valid progress template"),
    );
    pb.println(format!(
        "\nRunning {} mutants ({} jobs)...",
        candidates.len(),
        cfg.jobs
    ));

    let results = rt.block_on(run_mutants_async(
        candidates,
        &repo_root,
        cfg.timeout_secs,
        cfg.keep_temp_on_failure,
        cfg.jobs,
        cfg.include_dirs.clone(),
        Arc::clone(&pb),
    ));

    pb.finish_and_clear();

    let graph_idx = GraphIndex::build(&results);
    let include_dirs: Vec<Utf8PathBuf> = cfg
        .include_dirs
        .iter()
        .map(|s| Utf8PathBuf::from(s.as_str()))
        .collect();
    let abstractness_map = abstractness::compute(&repo_root, &include_dirs);
    let metrics_result = metrics::compute(&graph_idx, &abstractness_map);
    let cluster_result = graph_analysis::analyse(&graph_idx, &results);
    let fitness_report = fitness::evaluate_all(&graph_idx, &metrics_result, &cfg.fitness);
    terminal::print_report(
        &baseline,
        &results,
        &cluster_result,
        &metrics_result,
        &fitness_report,
    );

    let report = RunReport::build(&baseline, &results, &cluster_result);

    write_outputs(args, &graph_idx, &report);

    if args.fail_on_violations && fitness_report.has_errors() {
        std::process::exit(2);
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
    timeout_secs: u64,
    keep_on_fail: bool,
    jobs: usize,
    include_dirs: Vec<String>,
    pb: Arc<ProgressBar>,
) -> Vec<MutantResult> {
    let sem = Arc::new(Semaphore::new(jobs));
    let include_dirs = Arc::new(include_dirs);
    let mut join_set: JoinSet<MutantResult> = JoinSet::new();

    for candidate in candidates {
        let permit = Arc::clone(&sem)
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let repo = repo_root.to_path_buf();
        let pb = Arc::clone(&pb);
        let dirs = Arc::clone(&include_dirs);

        join_set.spawn(async move {
            let _permit = permit; // released when this task completes
            let result =
                run_mutant_async(candidate, &repo, timeout_secs, keep_on_fail, &dirs).await;
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

fn write_outputs(args: &RunArgs, graph_idx: &GraphIndex, report: &RunReport) {
    if let Err(e) = dot::write_dot(graph_idx, args.dot_out.as_path()) {
        eprintln!("error writing dot output: {e}");
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
