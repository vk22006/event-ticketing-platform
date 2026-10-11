# EventPro Rust Backend (Foundation)

This directory contains the Axum-based Rust backend foundation for **EventPro**, running alongside the existing Express.js 5 application during the migration process.

---

## 1. Project Structure

```
eventpro-rs/
├── .cargo/
├── .env.example              <- Environment template with placeholders
├── Cargo.lock                <- Pinned dependency lockfile
├── Cargo.toml                <- Crate manifest and dev profile optimizations
├── README.md                 <- Service documentation
├── src/
│   ├── lib.rs                <- Library interface exposing modules
│   ├── main.rs               <- Server entry point, signal handling & runtime setup
│   ├── config.rs             <- Environment configuration loader, redactor & validator
│   ├── error.rs              <- Unified AppError with HTTP status mapping & HTML error rendering
│   ├── state.rs              <- Shared AppState (MySqlPool, MiniJinja, AppConfig)
│   └── routes/
│       ├── mod.rs            <- Router assembly, static file mounting & middleware
│       └── health.rs         <- /health (liveness) and /ready (database probe) handlers
└── tests/
    └── foundation_tests.rs   <- Unit & integration test suite (7 tests)
```

---

## 2. Prerequisites & Toolchain

- **Rust**: 1.80+ (Verified on `rustc 1.93.0` / Cargo 1.93.0)
- **MySQL**: 8.0+ running on port `3306` with database `event_lifecycle`

---

## 3. Configuration & Environment Variables

Create `.env` inside `eventpro-rs/` or allow the backend to fall back to the root `.env`:

```bash
cp .env.example .env
```

| Variable | Default | Description |
| :--- | :--- | :--- |
| `PORT` | `3001` | Service port (coexists with Express on 3000) |
| `HOST` | `127.0.0.1` | Network interface binding |
| `APP_ENV` / `NODE_ENV` | `development` | Environment mode (`development`, `production`, `test`) |
| `DATABASE_URL` | None | Full MySQL connection URL (`mysql://user:pass@host:port/db`) |
| `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME` | Express values | Discrete connection variables (fallback if `DATABASE_URL` not set) |
| `SESSION_SECRET` | None | Encryption key for sessions (minimum 16 characters) |
| `PUBLIC_DIR` | `../public` | Directory containing CSS, JS, and uploads |

> **Security Note**: `AppConfig` implements a custom `Debug` formatter that redacts `DATABASE_URL` and `SESSION_SECRET` as `[REDACTED]`. Credentials are never exposed in logs.

---

## 4. Build & Test Commands

Run all commands from the `eventpro-rs/` directory:

```bash
# Verify formatting
cargo fmt --check

# Compile and check types
cargo check

# Run linter
cargo clippy -- -D warnings

# Execute test suite
cargo test
```

---

## 5. Running Alongside Express (Coexistence)

Both backends can run simultaneously on the same host:

1. **Terminal 1 (Express Backend)**:
   ```bash
   # From repository root
   npm start
   # -> Runs at http://localhost:3000
   ```

2. **Terminal 2 (Rust Backend)**:
   ```bash
   # From eventpro-rs/
   cargo run
   # -> Runs at http://localhost:3001
   ```

### Verification Endpoints:
- **Liveness probe**: `GET http://localhost:3001/health`
- **Readiness probe (verifies MySQL connectivity)**: `GET http://localhost:3001/ready`
- **Static assets**: `GET http://localhost:3001/css/main.css`
- **Uploaded media**: `GET http://localhost:3001/uploads/...`

---

## 6. Database Session Table

The Rust backend utilizes `tower-sessions-sqlx-store`. On startup, it verifies or creates the following table in MySQL:

```sql
CREATE TABLE IF NOT EXISTS tower_sessions (
    id VARCHAR(128) NOT NULL PRIMARY KEY,
    data BLOB NOT NULL,
    expiry_date TIMESTAMP NOT NULL,
    INDEX idx_tower_sessions_expiry (expiry_date)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```

Existing tables (`users`, `events`, `categories`, `seat_types`, `bookings`, `tickets`) remain untouched.
