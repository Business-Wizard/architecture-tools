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

**Architecture Wind Tunnel** (`awt`) is a mutation-based coupling analysis tool for Python codebases. It mutates Python code (add/rename/remove parameters, remove imports/modules), runs verifiers in ephemeral temp directories, and aggregates what breaks into coupling clusters — revealing unexpected dependencies and refactor candidates.

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
# Run mutation analysis on a Python project
./target/debug/awt run --repo <PATH_TO_PYTHON_PROJECT> [OPTIONS]

# Or via cargo
cargo run -p awt -- run --repo <PATH_TO_PYTHON_PROJECT> [OPTIONS]

# Show all available options
./target/debug/awt run --help
```

### Configuration

Create an `awt.toml` in your Python project root to configure the tool:

```toml
# Verifiers to run (at least one required)
verifiers = ["ruff", "basedpyright", "pytest"]

# Optional: mutation operators to apply
operators = [
  "add_required_parameter",
  "remove_parameter",
  "rename_parameter",
  "remove_import",
  "remove_module",
]

# Optional: maximum mutations to test
max_mutations = 100
```

### Example Workflow

```bash
# Navigate to the architecture-tools repo
cd /Users/josephwilson/repos/beach/architecture-tools

# Build the tool
cargo build -p awt

# Run on a sample Python project
./target/debug/awt run ../some-python-project/

# The tool will:
# 1. Load configuration from awt.toml
# 2. Scan Python files with tree-sitter
# 3. Generate mutation candidates
# 4. Run baseline verifiers (ruff, basedpyright, pytest)
# 5. Apply each mutation to a temp directory and test
# 6. Generate a terminal report showing coupling clusters
```

### Output

The tool produces:
- **Terminal Report**: Coupling analysis showing which functions/modules are tightly coupled
- **JSON Output** (optional): Machine-readable results for further analysis
- **Delta Report**: Before/after comparison of what breaks with each mutation

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
