# EventPro Benchmarking and Verification Suite

This directory contains the consolidated performance benchmark harnesses, statistical analysis engines, correctness verification suites, and raw experimental measurements for EventPro.

---

## Tooling Inventory

The benchmark tooling is organized into three distinct operational categories:

### 1. Correctness & Security Verification Suites

These automated scripts verify that functional parity, data integrity, and security guarantees remain intact under load without running heavy benchmark stress:

| Script | Command | Purpose |
| :--- | :--- | :--- |
| `validate_routes.js` | `npm run test:routes` | Verifies semantic HTML responses, HTTP 200 statuses, and authenticated attendee rendering for the Axum backend. |
| `run_booking_invariant_test.js` | `npm run test:booking` | Executes 30 transactional booking and cancellation cycles, asserting zero seat capacity leaks (seat invariance strictly preserved). |
| `test_session_writes.js` | `npm run test:sessions` | Verifies that public catalogue GET requests produce 0 session store writes in MySQL and checks authenticated session persistence. |
| `test_indexes.js` | `npm run test:indexes` | Validates MySQL `EXPLAIN` query execution plans to verify composite indexes (`idx_bookings_user_booked`, `idx_events_organizer_created`) are leveraged. |

To run all correctness checks in sequence:
```bash
npm run test
```

### 2. Canonical Performance Benchmarks

The benchmark suite features a unified, reproducible entry point that enforces binary integrity, isolated telemetry resets, and alternating execution order:

| Script | Command | Purpose |
| :--- | :--- | :--- |
| `run_reconciled_benchmark.js` | `npm run bench:reconciled` | **Canonical Unified Benchmark Runner**: Reconciles experimental harnesses; executes alternating 5-trial controlled tests (C=1, 10) and multi-trial pool tuning (10, 20, 30) with release binary SHA-256 verification and `/metrics/db/reset` isolation. |
| `run_phase3_validation.js` | `npm run bench:validation` | Five-round strictly alternating validation benchmark runner (AB, BA, AB, BA, AB) evaluating baseline vs optimized binaries across all workloads (C=1, 10, 25, 50). |
| `harness.js` | `node harness.js --target <express|axum> ...` | General-purpose CLI load generator and process resource monitor using `autocannon` and PowerShell `Get-Process`. |

### 3. Profiling, Diagnostics & Analysis

| Script | Command | Purpose |
| :--- | :--- | :--- |
| `measure_startup.js` | `npm run test:startup` | Measures cold process startup-to-readiness latency for the Axum release binary (5 clean trials), preserving historical comparison baselines in `results/startup.json`. |
| `audit_environment.js` | `npm run audit:env` | Audits MySQL server variables (`max_connections`, `innodb_buffer_pool_size`), Windows Defender status, competing processes, and open ports. Saves `results/environment_audit.json`. |
| `analyze_phase3_validation.js` | `npm run analyze:validation` | Computes statistical summaries (mean ± std dev, min/max, percentiles, query reduction) from the final 5-round alternating validation results. Saves `results/phase3_summary.json`. |
| `analyze.js` | `npm run analyze:comparison` | Generates full side-by-side metric comparison matrices from historical `raw_express.json` and `raw_axum.json`. Saves `results/comparison.csv` and `results/comparison_summary.json`. |

---

## Prerequisites

- **Node.js**: v20+ (tested on v24.13.0)
- **Rust Toolchain**: 1.80+ (tested on rustc 1.93.0)
- **MySQL**: 8.0+ running with the `event_lifecycle` and `tower_sessions` databases seeded
- **Operating System**: Windows 11 (ARM64 or x86_64)

```bash
cd benchmarks
npm install
```

---

## Running Correctness Verifications

Before benchmarking or after backend modifications, run the lightweight correctness suite:

```bash
# 1. Verify semantic HTML and authenticated route rendering
npm run test:routes

# 2. Verify transactional booking integrity and seat invariance (30 cycles)
npm run test:booking

# 3. Verify public GET requests produce zero session writes in MySQL
npm run test:sessions

# 4. Verify composite database indexes and query plans
npm run test:indexes
```

---

## Executing the Canonical Benchmark

The primary benchmark runner is `run_reconciled_benchmark.js`. It ensures reproducible results by:
- Recording the exact release binary SHA-256 hash.
- Enforcing `RUST_LOG=warn` and process pipe isolation to eliminate console logging overhead.
- Invoking `POST /metrics/db/reset` prior to each trial so database pool wait times and query durations are measured per trial without pooling artifacts.
- Alternating test order between configurations to prevent thermal or ordering bias.

To execute the reconciled benchmark:
```bash
npm run bench:reconciled
```

Results are saved to `benchmarks/results/reconciled_benchmark_results.json`.

---

## Statistical Analysis and Reporting

To regenerate structured summary tables and statistical aggregations from preserved raw results:

```bash
# Analyze the final Phase 3 alternating validation run (5 rounds, N=5 per config)
npm run analyze:validation

# Generate side-by-side Express vs Axum CSV and JSON summaries
npm run analyze:comparison
```

The canonical benchmark documentation, methodology, architectural tradeoffs, and performance tables are permanently documented in [`BENCHMARK_REPORT.md`](BENCHMARK_REPORT.md).
