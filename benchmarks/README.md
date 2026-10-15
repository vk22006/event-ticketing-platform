# EventPro Performance Benchmarking Suite

This directory contains the reproducible benchmark harness, analysis scripts, raw measurements, and comparison reports comparing the original Express.js backend with the replacement Rust/Axum backend.

## Prerequisites

- **Node.js**: v20+ (tested on v24.13.0)
- **Rust toolchain**: 1.80+ (tested on rustc 1.93.0)
- **MySQL**: 8.0+ running with the `event_lifecycle` database seeded
- **Operating System**: Windows 11 (ARM64 or x86_64)

## Architecture and Tooling

- **Load Generator**: `autocannon` v8.0.0 (high-performance async HTTP load testing using native Node streams).
- **Process Resource Monitor**: Windows PowerShell `Get-Process` sampling process CPU seconds and Private Working Set (`WorkingSet64`) memory during measurement windows.
- **Microbenchmarking & Startup**: Sub-millisecond timing of HTTP readiness probes from process launch.

## Benchmark Execution Guide

### 1. Install Benchmark Dependencies

```bash
cd benchmarks
npm install
```

### 2. Validating Route & Semantic Correctness

To verify that both backends return HTTP 200 with semantically valid HTML (and that authenticated routes properly render attendee data rather than redirects):

```bash
node benchmarks/validate_routes.js
```

### 3. Measuring Startup Latency

To measure cold startup-to-readiness latency (5 clean trials per backend):

```bash
node benchmarks/measure_startup.js
```

### 3. Benchmarking Express.js Backend

1. Start Express in production mode on port 3000:
   ```cmd
   set NODE_ENV=production
   set PORT=3000
   node app.js
   ```

2. Identify the process ID (PID):
   ```powershell
   Get-NetTCPConnection -LocalPort 3000 | Select-Object -Property OwningProcess
   ```

3. Run the load test harness:
   ```bash
   node benchmarks/harness.js --target express --url http://127.0.0.1:3000 --pid <PID> --concurrencies 1,10,25,50 --duration 10 --warmup 3 --runs 3
   ```

4. Stop the Express process when the run completes.

### 4. Benchmarking Axum Backend

1. Build the release binary:
   ```bash
   cd eventpro-rs
   cargo build --release
   ```

2. Start the release executable in production mode on port 3001:
   ```powershell
   $env:PORT="3001"
   $env:APP_ENV="production"
   .\eventpro-rs\target\release\eventpro.exe
   ```

3. Identify the process ID (PID):
   ```powershell
   Get-NetTCPConnection -LocalPort 3001 | Select-Object -Property OwningProcess
   ```

4. Run the load test harness:
   ```bash
   node benchmarks/harness.js --target axum --url http://127.0.0.1:3001 --pid <PID> --concurrencies 1,10,25,50 --duration 10 --warmup 3 --runs 3
   ```

5. Stop the Axum process when the run completes.

### 5. Generating Comparative Analysis

After both `results/raw_express.json` and `results/raw_axum.json` have been generated:

```bash
node benchmarks/analyze.js
```

This generates:
- `results/comparison.csv`: Full side-by-side metric matrix for all tested routes and concurrency levels.
- `results/comparison_summary.json`: Structured aggregations, standard deviations, and throughput/latency deltas.

## Workloads Evaluated

1. **Catalogue (`GET /events`)**: Public event list with category filtering (2 database queries: events + categories).
2. **Event Detail (`GET /events/1`)**: Public event view with seat tiers (2 database queries: event + seat tiers).
3. **Attendee Dashboard (`GET /dashboard`)**: Authenticated user home displaying booked tickets and profile.
4. **Booking History (`GET /bookings/my`)**: Authenticated history list of all user bookings.
5. **Operational Probes (`GET /health` & `GET /ready`)**: Axum microbenchmarks measuring non-blocking in-memory health and connection-pool ping latency.
