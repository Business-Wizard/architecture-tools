use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

use crate::config;
use crate::discovery;
use crate::model::{BaselineResult, VerifierStatus};
use crate::repo;
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

    println!("\n(mutation run not yet implemented)");
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
