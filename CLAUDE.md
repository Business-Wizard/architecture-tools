# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@CODE_CONVENTIONS.md

---

## Product: Architecture Wind Tunnel (`awt`)

**Technical category**: Static Architecture Analysis
**Team**: Better Bearings

Core thesis: *Import structure is a practical proxy for coupling.* The tool scans Python (and Rust) codebases, builds a coupling graph from import relationships, computes Stable Dependencies Principle (SDP) metrics, and reports center-of-gravity files, dependency violations, and refactor candidates — without running tests or modifying code.

---

## Development Commands

```bash
# Build
cargo build -p awt

# Run
cargo run -p awt

# Tests (all)
cargo test --workspace

# Single test
cargo test -p awt test_name_here

# Type check
cargo check --all-features --all-targets --workspace

# Lint (auto-fix)
cargo clippy --fix --all-features --allow-staged --allow-dirty

# Lint (strict, CI mode)
cargo clippy --all-targets --all-features -- -D warnings

# Format
cargo fmt --all

# Pre-commit (runs fmt + check + clippy)
prek
```

Version control uses **jj (Jujutsu)**, never git directly:

```bash
jj describe -m "feat(graph): add instability clustering" && jj new
jj status
jj log
```

---

## Architecture

### Current State

The crate at `crates/awt/` is a scaffold with module stubs (`controller`, `domain`, `feature`, `presenter`, `view_model`) not yet wired to the `awt` product. Start here to implement the target layout below.

### Current Module Layout

```
src/
  main.rs          # clap CLI entry: `awt inspect`
  cli.rs           # InspectArgs, command routing
  graph.rs         # module declarations for graph/
  report.rs        # module declarations for report/

  graph/
    coupling_graph.rs  # petgraph coupling model, GraphIndex, FileRole
    metrics.rs         # instability, SDP metrics
    object_graph.rs    # class-level dependency graph
    rules.rs           # violation detection rules
    violations.rs      # GraphViolation types

  report/
    dot.rs             # write coupling.dot
    objects_dot.rs     # write objects.dot
    sdp_flow.rs        # write sdp_flow.png (plotters)
    terminal.rs        # terminal violation output
```

### Hard Constraints

| Area | Choice |
|---|---|
| Implementation language | Rust only |
| Target languages | Python, Rust |
| UI | Terminal report + file outputs |
| History mining | Out of scope |
| MCP / API | Out of scope |
| Architecture labels | Out of scope |
| Scoring | Out of scope for v1 |

### Key Crates

| Need | Crate |
|---|---|
| CLI | `clap` |
| File walking | `ignore` |
| Paths | `camino` |
| Graph | `petgraph` |
| Charts | `plotters` |
| Terminal tables | `comfy-table` |
| Terminal color | `colored` |
| Serialization | `serde`, `serde_json`, `toml` |

### Clean Architecture Layers

The project follows the MVP pattern from the global `CLAUDE.md`:

- **domain** — pure data structures (no I/O, no error framework deps)
- **usecases** — orchestration; depends only on domain
- **presenter** — maps `Result<T, E>` from usecases → `ViewModel`
- **controller** — routes CLI args to usecases, passes result to presenter
- **views** — renders `ViewModel` (terminal); never imports usecases

### Pipeline

`awt inspect <PATH>` executes in order:
1. Walk source files (Python or Rust) via `ignore` → 2. Parse import relationships via `py-analyzer` / `rs-analyzer` → 3. Build coupling graph (`GraphIndex`) → 4. Compute SDP metrics → 5. Optionally detect violations → 6. Write `.dot` / `.png` outputs → 7. Print terminal report.

---

## Workspace Configuration

- **Rust edition**: 2024
- **Clippy**: `pedantic = "deny"` (workspace-wide)
- **Build artifacts**: `target/rust` (configured in `.cargo/config.toml`)
- **Dev profile**: overflow checks ON, 256 codegen-units
- **Release profile**: thin-LTO, panic=abort

Pre-commit hooks (`prek`) enforce: fmt, cargo check, clippy (fix then lint), conventional commits, trailing whitespace, TOML validity.

---

## Conventional Commits

Required format: `(chore|test|feat|fix|fixup|drop|build|docs|refactor)!?(\([a-z]+\))?: message`

Scopes to use: `awt`, `graph`, `report`, `cli`, `config`


<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Commit and push beads state** - This project uses `jj`, not `git`:
   ```bash
   bd dolt push
   jj describe -m "chore: update beads task state" && jj new
   ```
5. **Update issue status** — Close finished work, update in-progress items
6. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Use `jj commit` — never `git commit` or `git push`
- Never push to remote without explicit user instruction (per project conventions)
<!-- END BEADS INTEGRATION -->
