# Sample Order Processing Project

A minimal Python stub project for testing the **Architecture Wind Tunnel (AWT)** tool.

## Project Structure

```
sample-python-project/
├── src/                    # Source code
│   ├── __init__.py
│   ├── customer.py        # Customer domain model
│   ├── order.py           # Order domain and orchestration
│   └── billing.py         # Billing service (tightly coupled to Order)
├── tests/                 # Unit tests
│   ├── __init__.py
│   └── test_order.py      # Tests for Order module
├── awt.toml               # AWT configuration
└── pyproject.toml         # Python project metadata
```

## Domain Model

- **Customer**: Represents a customer (simple data holder)
- **Order**: Core domain entity with methods to confirm/cancel
- **BillingService**: Processes payments (depends on Order, creates coupling)

The coupling between `billing.py` and `order.py` is intentional—AWT will detect this when you add a required parameter to functions like `Order.get_customer_name()`.

## Running AWT

### Step 1: Install ruff in the nix shell

```bash
# In the architecture-tools repo's nix shell, install ruff
cd /Users/josephwilson/repos/beach/architecture-tools
direnv allow  # if needed
pip install ruff
```

Or add it to your flake.nix:

```nix
pkgs.ruff  # Add to buildInputs in devShells.default
```

### Step 2: Run AWT with dry-run (discovery only)

```bash
/Users/josephwilson/repos/beach/architecture-tools/target/debug/awt run \
  --repo /tmp/sample-python-project \
  --dry-run
```

This will:
- Scan all Python files
- Discover candidate functions for mutation
- Show counts without running verifiers

### Step 3: Run full mutation analysis

```bash
/Users/josephwilson/repos/beach/architecture-tools/target/debug/awt run \
  --repo /tmp/sample-python-project \
  --max-mutants 20
```

This will:
1. Load config from `awt.toml`
2. Run baseline verifier (ruff) to ensure project is clean
3. Discover mutation candidates (functions that can be modified)
4. Apply each mutation to a temp directory and run ruff
5. Aggregate failures into coupling clusters
6. Print terminal report showing which modules are tightly coupled

### Step 4: Save JSON output for comparison

```bash
/Users/josephwilson/repos/beach/architecture-tools/target/debug/awt run \
  --repo /tmp/sample-python-project \
  --max-mutants 20 \
  --json-out results.json
```

### Step 5: Compare two runs (delta report)

```bash
/Users/josephwilson/repos/beach/architecture-tools/target/debug/awt run \
  --repo /tmp/sample-python-project \
  --max-mutants 20 \
  --json-out results_new.json \
  --compare results.json
```

## Expected Behavior

When AWT adds a required parameter to `Order.get_customer_name()`:

- **billing.py** will fail (calls this method without the new parameter) → detected coupling
- **order.py** will fail (defines the function but doesn't pass the new parameter in internal calls)
- **test_order.py** will fail (tests call this method)

This reveals that:
1. `BillingService` is tightly coupled to the `Order` interface
2. The function is called from multiple places, making changes costly

## Troubleshooting

### `ruff: command not found`

Install ruff in your environment:
```bash
pip install ruff
```

Or ensure your nix shell includes it.

### `No such file or directory` errors for basedpyright/pytest

These are optional verifiers. To use them:
1. Install via pip: `pip install basedpyright pytest`
2. Or remove from `verifiers` list in `awt.toml`

### Config parse errors

Ensure `awt.toml` uses the correct format:
```toml
verifiers = ["ruff"]

[operators]
add_required_parameter = true

max_mutations = 50
```
