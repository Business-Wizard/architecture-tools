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
- **BillingService**: Processes payments (depends on Order, creating a coupling edge)

The coupling between `billing.py` and `order.py` is intentional — AWT will detect it as an import dependency edge and report instability metrics for both modules.

## Running AWT

### Step 1: Build AWT

```bash
cd /Users/josephwilson/repos/beach/architecture-tools
cargo build -p awt
```

### Step 2: Run static analysis

```bash
/Users/josephwilson/repos/beach/architecture-tools/target/debug/awt inspect \
  /path/to/sample-python-project
```

This will:
- Scan all Python files
- Parse import relationships
- Build a coupling graph
- Compute stability metrics (SDP)
- Write `coupling.dot`, `sdp_flow.png`, and `objects.dot`

## Expected Behavior

When AWT inspects this project:

- **billing.py** imports from **order.py** → coupling edge detected
- **order.py** has high afferent coupling (depended on by billing + tests) → low instability (stable)
- **billing.py** has low afferent coupling → high instability (unstable)

This confirms the expected dependency direction: unstable modules depend on stable ones.
