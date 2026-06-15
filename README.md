# Architecture Mutation Testing

template for future data projects

## Getting Started - Development Environment

This project uses **Nix Flakes** for reproducible development environments:

```bash
# Install direnv (if you haven't already)
brew install direnv

# Enable direnv for this repo
direnv allow

# direnv will automatically load the development environment when you enter the directory
cd .  # or just wait a moment for the environment to load
```

The development environment includes:
- Git, Jujutsu (version control)
- Pre-commit hooks
- UV (Python package manager)
- Rust toolchain (cargo, rustup)
- VSCode
- Docker and Podman
- jq

### What Happens Automatically

When you `cd` into this directory:
1. **Nix Environment**: direnv loads the flake, installing all declared dependencies
2. **Prompt appears immediately** - you can start typing right away
3. **Python venv**: If you have a `pyproject.toml` and `.venv` exists, it activates before your first command (via zsh precmd hook)
4. **All tools available**: Git, Rust, cargo, uv, Docker, Podman, and more are in your PATH

### Environment Loading Speed

The direnv setup is optimized to never block your typing:
- First direnv load: ~5-10 seconds (downloads/builds dependencies)
- Subsequent loads: <100ms (cached by nix-direnv)
- **Prompt appears instantly** (no blocking)
- venv activation: Happens silently before your first command (<50ms)

Key design:
- Nix environment loads synchronously (required for dependencies)
- venv activation deferred to zsh precmd hook (runs after prompt, before command)
- Result: Instant prompt + venv active before any command runs

### Managing Podman VM (macOS)

Podman is included in the project dependencies, but VM startup is optional and personal preference:

**Option 1: Manual Start**

```bash
podman machine start
```

**Option 2: Automatic (add to your ~/.zshrc)**
This is a personal choice - not enforced by the project. Add this to your shell config:

```bash
# Auto-start Podman VM once per shell session
if command -v podman &> /dev/null; then
  podman_vm_state=$(podman machine info --format json 2>/dev/null | jq -r '.Host.MachineState' 2>/dev/null)
  if [[ "$podman_vm_state" != "Running" ]]; then
    echo "🐳 Starting Podman VM..." >&2
    podman machine start >/dev/null 2>&1 &
  fi
fi
```

This approach ensures:
- All team members have Podman/Docker from the flake.nix
- VM startup automation is optional/personal (not forced on everyone)
- Clean separation between project dependencies and user preferences

### Using with Traditional `nix develop`

If you prefer not to use direnv, you can manually enter the shell:

```bash
nix flake update  # optional: update to latest nixpkgs
nix develop
```

## Rust workspace support

- **Toolchain pinning**: `rust-toolchain.toml` locks the channel to stable and keeps `rustfmt`/`clippy` available so editor tooling is always consistent.
- **Shared workspace**: the root `Cargo.toml` designates `rust/data-app` as the default member and keeps a single `Cargo.lock`, which makes multi-crate changes atomic.
- **Faster builds**: `.cargo/config.toml` reuses `target/rust`, enables incremental builds, and lifts developer `codegen-units`, while Linux builds pass `-fuse-ld=mold` for quicker linking; install `mold` plus a compatible linker (e.g. `cc`) before enabling it in your environment.
- **Profiles tuned for iteration & CI releases**: dev builds keep overflow checks on with high parallelism, release builds use thin-LTO/panic-abort defaults to keep iteration fast without sacrificing correctness.
- **Getting started**: run `cargo run -p data_app` from the repo root to exercise the template; add additional members under `rust/` and the workspace `members` array as needed.

## Architecture Wind Tunnel (AWT) Tool

**Architecture Wind Tunnel** (`awt`) is a static architecture analysis tool for Python and Rust codebases. It scans source files, builds a coupling graph from import relationships, computes stability metrics, and reports architectural violations — without running tests or modifying code.

### Building AWT

The project is already configured in your Nix shell. To build:

```bash
# Build the awt binary
cargo build -p awt

# Or run directly (builds if needed)
cargo run -p awt --
```

### Using AWT

Once built, the `awt` binary is available as:

```bash
# Inspect a Python or Rust project
./target/debug/awt inspect <PATH_TO_PROJECT> [OPTIONS]

# Or via cargo
cargo run -p awt -- inspect <PATH_TO_PROJECT> [OPTIONS]

# Show all available options
./target/debug/awt inspect --help
```

**Key Options:**
- `<PATH>` — Path to the project to inspect (required)
- `--language <python|rust>` — Language of the codebase (default: python)
- `--violations` — Analyse structural coupling problems (cycles, hubs, god modules)
- `--fail-on-violations` — Exit with code 2 if any graph violations are found
- `--dot-out <PATH>` — Write coupling graph to a .dot file (default: coupling.dot)
- `--sdp-out <PATH>` — Write SDP dependency-flow chart to a PNG file (default: sdp_flow.png)
- `--objects-out <PATH>` — Write object-level class graph to a .dot file (default: objects.dot)

### Example Workflow

```bash
# Navigate to the architecture-tools repo
cd /Users/josephwilson/repos/beach/architecture-tools

# Build the tool
cargo build -p awt

# Inspect a Python project
./target/debug/awt inspect ../some-python-project/

# The tool will:
# 1. Scan Python files and parse import relationships
# 2. Build a coupling graph
# 3. Compute stability metrics (SDP)
# 4. Write coupling.dot, sdp_flow.png, objects.dot
```

### Output

The tool produces:

- **Terminal report**: Coupling violations, hub files, and instability metrics
- **coupling.dot**: Full coupling graph (render with graphviz `dot -Tpng`)
- **sdp_flow.png**: Stable Dependencies Principle flow chart
- **objects.dot**: Object-level class relationship graph

#### Top Centers of Gravity
Files with the most coupling. Shows:
- **Source code affected**: How many other source files break when this file is mutated
- **Test code affected**: How many test files break when this file is mutated
- **Package**: Which package the file belongs to

#### Coupling Components
A measure of how interconnected the coupling is:
- **1 component** = All coupling is in one tightly-interconnected web
- **2+ components** = Coupling is fragmented into separate groups

Lower numbers (approaching 1) indicate tighter coupling.

#### Unexpected Cross-Package Coupling
Shows mutations that break code in **unexpected** packages (e.g., a `src/` module mutation breaking a `tests/` file when they shouldn't be coupled). If this section is empty, all detected coupling is expected and contained.

**JSON Output** (optional): Use `--json-out <PATH>` for machine-readable results
**Delta Report**: Use `--compare <PATH>` to compare against a previous run

### Testing & Development

```bash
# Run all tests
cargo test --workspace

# Run specific test
cargo test -p awt test_name_here

# Type check
cargo check --all-features --all-targets --workspace

# Lint (auto-fix)
cargo clippy --fix --all-features --allow-staged --allow-dirty

# Format code
cargo fmt --all

# Pre-commit checks (fmt + check + clippy)
prek
```
