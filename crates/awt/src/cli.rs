use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use ignore::WalkBuilder;

use crate::graph::{coupling_graph::GraphIndex, metrics};
use crate::report::{dot, sdp_flow, terminal};

#[derive(Parser)]
#[command(
    name = "awt",
    about = "Architecture Wind Tunnel — static graph analysis for Python codebases"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Inspect(InspectArgs),
}

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum Language {
    #[default]
    Python,
    Rust,
}

#[derive(Parser, Debug)]
pub struct InspectArgs {
    #[arg(help = "Path to the package to inspect")]
    pub path: Utf8PathBuf,

    #[arg(long, default_value = "python", help = "Language of the codebase")]
    pub language: Language,

    #[arg(
        long,
        default_value_t = 120,
        help = "Timeout in seconds (reserved for future use)"
    )]
    pub timeout_secs: u64,

    #[arg(
        long,
        help = "Analyse structural coupling problems (cycles, hubs, god modules)"
    )]
    pub violations: bool,

    #[arg(long, help = "Exit with code 2 if any graph violations are found")]
    pub fail_on_violations: bool,

    #[arg(
        long,
        default_value = "coupling.dot",
        help = "Write coupling graph to this .dot file"
    )]
    pub dot_out: Utf8PathBuf,

    #[arg(
        long,
        default_value = "sdp_flow.png",
        help = "Write SDP dependency-flow chart to this PNG file"
    )]
    pub sdp_out: Utf8PathBuf,
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Inspect(ref args) => run_inspect_command(args),
    }
}

fn run_inspect_command(args: &InspectArgs) {
    let (analyzer, namer): (
        Box<dyn lang_core::LanguageAnalyzer>,
        Box<dyn lang_core::ModuleNamer>,
    ) = match args.language {
        Language::Python => (
            Box::new(py_analyzer::PythonAnalyzer),
            Box::new(py_analyzer::PythonAnalyzer),
        ),
        Language::Rust => (
            Box::new(rs_analyzer::RustAnalyzer),
            Box::new(rs_analyzer::RustAnalyzer),
        ),
    };

    match analyzer.module_deps(args.path.as_std_path()) {
        Ok(module_deps) => {
            let violations = if args.violations {
                let v = ::graph_analysis::analyze(&module_deps);
                terminal::print_graph_violations_section(&v, &args.path);
                v
            } else {
                vec![]
            };

            let source_files = collect_source_files(&args.path, namer.file_extension().as_str());
            let graph_idx =
                GraphIndex::build_from_module_deps(&module_deps, &source_files, namer.as_ref());
            let metrics_result = metrics::compute(&graph_idx);

            if let Err(e) = dot::write_dot(&graph_idx, &metrics_result, args.dot_out.as_path()) {
                eprintln!("warning: could not write dot output: {e}");
            }
            if let Err(e) =
                sdp_flow::write_sdp_flow(&graph_idx, &metrics_result, args.sdp_out.as_path())
            {
                eprintln!("warning: could not write SDP flow chart: {e}");
            }

            if args.fail_on_violations && !violations.is_empty() {
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

fn collect_source_files(root: &Utf8PathBuf, ext: &str) -> Vec<Utf8PathBuf> {
    WalkBuilder::new(root.as_std_path())
        .hidden(false)
        .build()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|e| e == ext))
        .filter_map(|e| {
            let p = camino::Utf8PathBuf::try_from(e.into_path()).ok()?;
            p.strip_prefix(root).ok().map(camino::Utf8Path::to_owned)
        })
        .collect()
}
