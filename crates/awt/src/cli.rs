use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use rayon::prelude::*;

use crate::config;
use crate::config::Config;
use crate::discovery;
use crate::failures::{basedpyright, pytest, ruff};
use crate::model::{
    BaselineResult, Candidate, FailureCategory, FailureEvent, MutantResult, MutantStatus,
    VerifierKind, VerifierStatus,
};
use crate::mutations::add_parameter;
use crate::repo;
use crate::runner::temp_repo::TempRepo;
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

    #[arg(long, help = "Number of parallel mutation jobs")]
    pub jobs: Option<usize>,

    #[arg(long, help = "Keep temp directory when a mutation run fails")]
    pub keep_temp_on_failure: bool,

    #[arg(long, help = "Write full run results to this JSON file")]
    pub json_out: Option<Utf8PathBuf>,

    #[arg(long, help = "Compare against a previous run JSON for delta report")]
    pub compare: Option<Utf8PathBuf>,

    #[arg(
        long,
        help = "Discover candidates and print counts without running mutations"
    )]
    pub dry_run: bool,
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run(args) => run_command(args),
    }
}

fn run_command(args: RunArgs) {
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

    let verifiers = VerifierSet::new(cfg.timeout_secs);

    println!("Baseline:");
    let baseline = run_baseline(&verifiers, &repo_root);
    print_baseline(&baseline);

    if !baseline.all_pass() {
        eprintln!("\nBaseline failed. Aborting.");
        std::process::exit(1);
    }

    let discovery = match discovery::discover(&repo_root, &cfg) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error during discovery: {e}");
            std::process::exit(1);
        }
    };

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

    println!(
        "\nRunning {} mutants ({} jobs)...",
        candidates.len(),
        cfg.jobs
    );

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.jobs)
        .build()
        .expect("failed to build rayon pool");

    let keep_on_fail = cfg.keep_temp_on_failure;
    let timeout = cfg.timeout_secs;
    let repo_ref = &repo_root;

    let results: Vec<MutantResult> = pool.install(|| {
        candidates
            .into_par_iter()
            .map(|candidate| run_mutant(candidate, repo_ref, timeout, keep_on_fail))
            .collect()
    });

    print_mutation_summary(&results);
}

fn run_mutant(
    candidate: Candidate,
    repo_root: &std::path::Path,
    timeout_secs: u64,
    keep_on_fail: bool,
) -> MutantResult {
    let source = match std::fs::read(repo_root.join(candidate.file.as_str())) {
        Ok(s) => s,
        Err(e) => {
            return MutantResult {
                candidate,
                status: MutantStatus::Invalid,
                local_failures: vec![make_runner_failure(&candidate_stub(), &e.to_string())],
                external_failures: vec![],
            };
        }
    };

    let mutated = match add_parameter::apply(&source, &candidate) {
        Ok(m) => m,
        Err(e) => {
            return MutantResult {
                candidate,
                status: MutantStatus::Invalid,
                local_failures: vec![make_runner_failure(&candidate_stub(), &e.to_string())],
                external_failures: vec![],
            };
        }
    };

    let temp = match TempRepo::copy_from(repo_root) {
        Ok(t) => t,
        Err(e) => {
            return MutantResult {
                candidate,
                status: MutantStatus::Invalid,
                local_failures: vec![make_runner_failure(&candidate_stub(), &e.to_string())],
                external_failures: vec![],
            };
        }
    };

    if let Err(e) = temp.write_mutated_file(candidate.file.as_str(), &mutated) {
        return MutantResult {
            candidate,
            status: MutantStatus::Invalid,
            local_failures: vec![make_runner_failure(&candidate_stub(), &e.to_string())],
            external_failures: vec![],
        };
    }

    let timeout = std::time::Duration::from_secs(timeout_secs);
    let temp_path = temp.path().to_path_buf();
    let id = &candidate.id;

    let mut all_failures: Vec<FailureEvent> = vec![];
    all_failures.extend(ruff::run_and_parse(id, &temp_path, timeout).unwrap_or_default());
    all_failures.extend(basedpyright::run_and_parse(id, &temp_path, timeout).unwrap_or_default());
    all_failures.extend(pytest::run_and_parse(id, &temp_path, timeout).unwrap_or_default());

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
        .filter(|f| f.is_local)
        .cloned()
        .collect();
    let external_failures: Vec<FailureEvent> =
        all_failures.into_iter().filter(|f| !f.is_local).collect();

    MutantResult {
        candidate,
        status,
        local_failures,
        external_failures,
    }
}

fn candidate_stub() -> camino::Utf8PathBuf {
    camino::Utf8PathBuf::from("unknown")
}

fn make_runner_failure(_file: &camino::Utf8PathBuf, msg: &str) -> FailureEvent {
    use crate::model::MutantId;
    FailureEvent {
        mutant_id: MutantId("unknown".into()),
        command: VerifierKind::Ruff,
        file: camino::Utf8PathBuf::from("unknown"),
        line: None,
        column: None,
        symbol: None,
        category: FailureCategory::Unknown,
        message: msg.to_string(),
        is_local: true,
    }
}

fn print_mutation_summary(results: &[MutantResult]) {
    let breaks = results
        .iter()
        .filter(|r| r.status == MutantStatus::Breaks)
        .count();
    let survives = results
        .iter()
        .filter(|r| r.status == MutantStatus::Survives)
        .count();
    let invalid = results
        .iter()
        .filter(|r| r.status == MutantStatus::Invalid)
        .count();

    println!("\nMutation Summary:");
    println!("  breaks:   {breaks}");
    println!("  survives: {survives}");
    println!("  invalid:  {invalid}");
}

fn run_baseline(verifiers: &VerifierSet, repo: &std::path::Path) -> BaselineResult {
    let ruff = verifiers
        .run_ruff(repo)
        .unwrap_or_else(|e| VerifierStatus::Fail(vec![format!("runner error: {e}")]));
    let basedpyright = verifiers
        .run_basedpyright(repo)
        .unwrap_or_else(|e| VerifierStatus::Fail(vec![format!("runner error: {e}")]));
    let pytest = verifiers
        .run_pytest(repo)
        .unwrap_or_else(|e| VerifierStatus::Fail(vec![format!("runner error: {e}")]));
    BaselineResult {
        ruff,
        basedpyright,
        pytest,
    }
}

fn print_baseline(b: &BaselineResult) {
    print_verifier_status("ruff", &b.ruff);
    print_verifier_status("basedpyright", &b.basedpyright);
    print_verifier_status("pytest", &b.pytest);
}

fn print_verifier_status(name: &str, status: &VerifierStatus) {
    match status {
        VerifierStatus::Pass => println!("  {name}: pass"),
        VerifierStatus::Fail(lines) => {
            println!("  {name}: {} existing errors", lines.len());
        }
    }
}
