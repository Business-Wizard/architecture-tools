# faa-audit Project Configuration

This project follows the global coding conventions defined in the shared configuration.

## Global Configuration

@~/.claude/CLAUDE.md
@../faa-audit-capture-mvp-data-generator/output

The global config includes:
- Test writing conventions (small/medium/large test classification)
- Naming conventions (CamelCase acronyms: `Pii`, `Json`, `Http`)
- Code quality standards (`returns` package types, DTOs, no magic numbers)
- Clean Architecture patterns (MVP, layered dependencies)
- Python-specific practices (conventional commits, pre-commit hooks)

See the global config file for complete details.

## Coding Conventions

### Test Writing Conventions

1. **Test Naming (Backend Python)**: Use `test_<scenario>_should_<expected_behavior>` format
2. **Test Naming (Frontend JS)**: Use `it("should <expected behavior>")` inside `describe("<Component>")` blocks — see `docs/frontend/style-guide.md`
3. **Single Assert Pattern**: Tests should have ONE assertion comparing objects — `assert actual == expected` (Python) / `expect(actual).toEqual(expected)` (JS)
4. **Use Fixtures for IO**: Always use `tmp_path` fixture for file operations (Python only)
5. **No Verbose Comments**: Test names explain intent; no docstrings needed
6. **Test Size Awareness**: Classify tests using Abseil's size categories (small, medium, large); use these terms, not "unit/integration/e2e"

### Naming Conventions

1. **CamelCase Acronyms**: All acronyms in class/object names use only the first letter capitalized
   - `Pii` (not `PII`), `Json` (not `JSON`), `Http` (not `HTTP`)

### Code Quality

1. **No Magic Numbers**: Use enums for constants
2. **Avoid Primitives for Domain Concepts**: Create DTOs with `__str__` or `to_json`
3. **Use `returns` Package Types**:
   - Use `Result[T, E]` instead of raising exceptions
   - Use `Maybe[T]` for all optional values — domain, usecases, commands (including `UpdateXxxCommand`), presenters, repository implementations, controllers
   - `T | None` is permitted **only** in the two infrastructure serialization modules:
     - `view_model.py` — Pydantic models at the HTTP/frontend boundary
     - `feature/postgres_repo/_models.py` — SQLAlchemy ORM rows at the datastore boundary
   - **Never** use `Maybe[T] | None` — this creates a false three-state type and is always a design error
   - `UpdateXxxCommand` fields are **not** an exception — use `Maybe[T] = Nothing` as the "don't update" sentinel
4. **Configuration Over Hardcoding**: Make dependencies injectable (e.g., `root_dir: Maybe[Path]` parameter)
5. **Code Organization**: Order by dependency — higher abstractions first; if A uses B, A comes before B in the file

### Conventional Commits

Required format: `(chore|test|feat|fix|fixup|drop|build|docs|refactor)!?(\([a-z]+\))?: message`

Examples: `feat(agents): add self-improving pattern`, `fix(cli): handle missing dataset`

**`drop` is a special type**: marks a deliberately temporary commit pushed to expose a bug in CI/production — intended for reversion once the bug is found. Do NOT use it for removing code, dependencies, or features.

### Version Control

**CRITICAL: Always use `jj` (Jujutsu), never use `git` directly.**

- Never add "Co-Authored-By" lines to commit messages
- Use `jj describe -m "message" && jj new` for commits

### Architecture & Design Patterns

**Layer Structure:**

```
domain/       - Core business logic (no dependencies)
usecases/     - Application business rules; orchestrates domain
presenter/    - Converts domain results to ViewModels
controller/   - Routes requests to usecases
views/        - Infrastructure adapters (never imports usecases)
```

**Dependency Flow:** `views → presenter → usecases → domain`, with `controller → usecases + presenter`

**MVP Pattern:**
- Presenter transforms `Result<DomainEntity, Error>` from UseCases into ViewModels
- Views render ViewModels only — no business logic
- ViewModels are serializable; domain entities are not

## Project-Specific Conventions

### Domain Context

This project implements an FAA audit capture system following Clean Architecture principles:

- **Domain Layer**: Core audit entities and value objects
- **Use Cases Layer**: Audit capture and processing business logic
- **Presenter Layer**: Transform audit results to view models
- **Controller Layer**: Route audit requests to use cases
- **Views Layer**: Infrastructure adapters (JSON, CLI output)

### Package Structure

Backend follows Clean Architecture with layered organization in `apps/backend/src/`:
- Domain → Use Cases → Presenters → Controllers → Views
- See source tree for detailed structure
- Before modifying a file, use `loctree slice` to understand its dependencies and consumers

### Testing Strategy

- Use `tmp_path` fixtures for file system operations
- **Backend**: Test names follow `test_<scenario>_should_<expected_behavior>` format
- **Frontend**: Test names follow `it("should <expected behavior>")` format (see `docs/frontend/style-guide.md`)
- Single assertion per test comparing `actual == expected`
- Classify test sizes explicitly (small, medium, large per Abseil conventions)
- `nx test:small backend` for small (hermetic) tests, `nx test:medium backend` for medium tests

## AI Agent Tools (MCP)

This project provides Model Context Protocol (MCP) servers for AI agents. **These tools are for agents only** — human developers don't need to learn or use these commands.

### loctree MCP (Code Navigation & Architecture Analysis)

**When to use:** Any time you're navigating, modifying, or understanding the codebase — not just during formal workflow phases.

**Rationale:** See ADR-010. Provides 10-20x token efficiency vs Grep for structural queries, critical for our aggressive MVP timeline.

**Available tools:**
1. **`for_ai`** — Full project overview with health and architectural insights (start here)
2. **`slice`** — Extract file + dependencies + consumers (holographic context slice)
3. **`find`** — Cross-term search (find where concepts intersect across codebase)
4. **`impact`** — Show what breaks before refactoring (dependency graph analysis)
5. **`dead`** — Detect unused exports across the codebase
6. **`tree`** — Directory structure visualization

**Language support:**
- Python (~20% false-positive rate, acceptable for context gathering)
- TypeScript/JavaScript (~10-20% false-positive rate, includes JSX/TSX)

**When to prefer loctree over built-in tools:**

| Task | Use loctree | Use Grep/Glob |
|------|-------------|---------------|
| "What depends on this module?" | ✅ `slice` or `impact` | ❌ (requires manual graph traversal) |
| "Is this function used anywhere?" | ✅ Dead export detection | ⚠️ Grep works but has false positives |
| "Find circular dependencies" | ✅ `impact` (Tarjan's algorithm) | ❌ (not feasible manually) |
| "What breaks if I change X?" | ✅ `impact` (instant) | ❌ (manual analysis) |
| "Find string 'TODO' in codebase" | ❌ (overkill) | ✅ `grep -r "TODO"` |
| "Find files named *.test.py" | ❌ (wrong tool) | ✅ `glob "**/*.test.py"` |
| "Search for exact string match" | ❌ (overkill) | ✅ Grep is faster |

**Example MCP usage patterns:**

```text
# Starting work in an unfamiliar area
1. Use loctree `for_ai` for a full project overview
2. Use loctree `find` to locate files related to the concept you're working on
3. Use loctree `slice` on the key file to see its dependencies and consumers

# Before modifying a file
1. Use loctree `slice <file>` to understand what depends on it
2. Use loctree `impact <file>` to see the full blast radius of changes

# Before refactoring a Clean Architecture layer
1. Use loctree `slice apps/backend/src/domain/audit.py`
2. Verify no upward dependencies (domain → use cases is a violation)
3. Use loctree `impact` to confirm affected files before proceeding
```

**Token efficiency:** loctree returns structured JSON (paths, symbols, dependencies) instead of full file contents, saving 10-20x tokens on architectural queries. Critical for complex `/design` and `/imp` workflows.
