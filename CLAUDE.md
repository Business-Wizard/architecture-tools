# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@CODE_CONVENTIONS.md

---

## Product: Architecture Wind Tunnel (`awt`)

**Technical category**: Architecture Mutation Testing
**Team**: Better Bearings

Core thesis: *Breakage propagation is a practical proxy for coupling.* The tool mutates Python code (add/rename/remove parameters, remove imports/modules), runs verifiers in ephemeral temp directories, and aggregates what breaks into coupling clusters. It reports center-of-gravity files, unintended dependencies, and refactor candidates — without scores, labels, or VCS history.

MVP thesis, success criteria, pipeline, and mutation operator specs are in `PROMPT.md`. Read it before building anything.

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
jj describe -m "feat(awt): add mutation discovery" && jj new
jj status
jj log
```

---

## Architecture

### Current State

The crate at `crates/awt/` is a scaffold with module stubs (`controller`, `domain`, `feature`, `presenter`, `view_model`) not yet wired to the `awt` product. Start here to implement the target layout below.

### Target Module Layout (from `PROMPT.md` §6)

```
src/
  main.rs          # clap CLI entry: `awt run`
  cli.rs
  config.rs        # awt.toml loading
  repo.rs          # repo root resolution
  discovery.rs     # scan Python files, parse with tree-sitter
  python_ast.rs    # tree-sitter query helpers (find functions, imports, byte ranges)

  mutations/       # one file per operator
    add_parameter.rs
    rename_parameter.rs
    remove_parameter.rs
    remove_import.rs
    remove_module.rs

  runner/
    temp_repo.rs   # copy repo → tempfile dir, apply mutation
    command.rs     # std::process::Command wrappers
    verifier.rs    # run ruff / basedpyright / pytest, parse stdout

  failures/
    ruff.rs        # parse ruff JSON output
    basedpyright.rs
    pytest.rs

  graph/
    coupling_graph.rs  # petgraph model
    clustering.rs

  report/
    terminal.rs    # comfy-table output
    summary.rs

  model.rs         # shared domain types: MutantId, CandidateKind, BreakageRecord, etc.
```

### Hard Constraints (from `PROMPT.md` §4)

| Area | Choice |
|---|---|
| Implementation language | Rust only |
| Target language | Python only |
| Python style | Typed Python |
| Type checker | basedpyright |
| Test runner | pytest |
| Linter | ruff |
| Package runner | uv |
| Execution model | Ephemeral temp directories |
| UI | Terminal report only |
| History mining | Out of scope |
| MCP / API | Out of scope |
| Architecture labels | Out of scope |
| Scoring | Out of scope for v1 |

### Key Crates to Add

| Need | Crate |
|---|---|
| CLI | `clap` |
| File walking | `ignore` or `walkdir` |
| Paths | `camino` |
| Python parsing | `tree-sitter`, `tree-sitter-python` |
| Temp dirs | `tempfile` |
| Graph | `petgraph` |
| Parallelism | `rayon` |
| Terminal tables | `comfy-table` |
| Terminal color | `anstyle` or `colored` |
| Serialization | `serde`, `serde_json`, `toml` |
| Stable IDs | `sha2` (hash-based) |

### Clean Architecture Layers

The project follows the MVP pattern from the global `CLAUDE.md`:

- **domain** — pure data structures (no I/O, no error framework deps)
- **usecases** — orchestration; depends only on domain
- **presenter** — maps `Result<T, E>` from usecases → `ViewModel`
- **controller** — routes CLI args to usecases, passes result to presenter
- **views** — renders `ViewModel` (terminal); never imports usecases

### Pipeline (from `PROMPT.md` §8)

`awt run` executes in order:
1. Load config → 2. Scan Python files → 3. Parse with tree-sitter → 4. Discover candidates → 5. Rank/select → 6. **Baseline verifier run** (abort if failing) → 7. For each mutant: copy repo, apply mutation, run ruff + basedpyright + pytest, parse failures → 8. Build coupling graph → 9. Cluster → 10. Print terminal report → 11. Optionally emit JSON.

Baseline must pass before mutation runs begin.

### Mutation Operators

Best first operator: **add required parameter** — appends `awt_required_probe: object` to a function signature. This reveals all call sites. See `PROMPT.md` §10 for full operator specs and skip constraints (e.g. skip functions with `*args`, `**kwargs`, `@overload`).

### Candidate Identity

Each candidate has a stable dot-path ID for before/after comparison:

```
src.domain.order.Order.__init__:add_required_parameter:customer_id
```

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

Scopes to use: `awt`, `mutations`, `runner`, `graph`, `report`, `cli`, `config`


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
