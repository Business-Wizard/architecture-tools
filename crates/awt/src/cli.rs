use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use ignore::WalkBuilder;

use crate::config::MainSequenceConfig;
use crate::graph::{abstractness, coupling_graph::GraphIndex, metrics};
use crate::report::{chart, dot, sdp_flow, terminal};

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

#[derive(Parser, Debug)]
pub struct InspectArgs {
    #[arg(help = "Path to the Python package to inspect")]
    pub path: Utf8PathBuf,

    #[arg(long, default_value_t = 120, help = "Timeout in seconds for each tool")]
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
}

pub fn run() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Inspect(ref args) => run_inspect_command(args),
    }
}

fn run_inspect_command(args: &InspectArgs) {
    let timeout = std::time::Duration::from_secs(args.timeout_secs);
    let result = py_analyzer::inspect_with_timeout(args.path.as_std_path(), timeout);
    match result {
        Ok(inspect) => {
            let violations = if args.violations {
                let v = ::graph_analysis::analyze(&inspect);
                terminal::print_graph_violations_section(&v, &args.path);
                v
            } else {
                vec![]
            };

            let source_files = collect_source_files(&args.path);
            let graph_idx = GraphIndex::build_from_module_deps(&inspect.module_deps, &source_files);
            let include_dirs = vec![args.path.clone()];
            let abstractness_map = abstractness::compute(args.path.as_std_path(), &include_dirs);
            let metrics_result = metrics::compute(&graph_idx, &abstractness_map);

            let main_seq = MainSequenceConfig::default();
            if let Err(e) = dot::write_dot(&graph_idx, &metrics_result, args.dot_out.as_path()) {
                eprintln!("warning: could not write dot output: {e}");
            }
            if let Err(e) = chart::write_chart(&metrics_result, &main_seq, args.chart_out.as_path())
            {
                eprintln!("warning: could not write chart: {e}");
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

fn collect_source_files(root: &Utf8PathBuf) -> Vec<Utf8PathBuf> {
    WalkBuilder::new(root.as_std_path())
        .hidden(false)
        .build()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "py"))
        .filter_map(|e| {
            let p = camino::Utf8PathBuf::try_from(e.into_path()).ok()?;
            p.strip_prefix(root).ok().map(camino::Utf8Path::to_owned)
        })
        .collect()
}
