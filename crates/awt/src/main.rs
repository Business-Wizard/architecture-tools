mod cli;
mod config;
mod discovery;
mod failures;
mod graph;
mod model;
mod mutations;
mod python_ast;
mod repo;
mod report;
mod runner;

fn main() {
    cli::run();
}
