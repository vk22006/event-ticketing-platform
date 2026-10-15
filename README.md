# EventPro - An End-to-End Event Lifecycle & Ticketing Platform

EventPro is a full-stack event management and ticketing platform designed to manage the entire lifecycle of events—from initial planning and publishing to seat reservation, ticket generation, and post-booking management. The platform provides tailored workflows for three primary user personas: Attendees, Event Organizers, and Platform Administrators.

![home page](img/home_page.png)

The repository houses two backend implementations that share the same database schema, client assets, and business rules:
- **Rust / Axum Backend (`eventpro-rs/`)**: A modern, memory-safe backend built with Axum 0.7, Tokio, MiniJinja server-side templates, and SQLx. It enforces strict database transaction boundaries, row-level concurrency locks, and bounded multipart memory limits.
- **Node.js / Express Backend (Root)**: The baseline Express.js 5 application using EJS templates and MySQL2.

Both implementations connect to the same MySQL 8 database, allowing side-by-side local evaluation, zero-downtime canary rollouts, or immediate rollback if needed.

---

## Core Features and User Workflows

### Attendees
- **Event Catalogue & Discovery**: Browse published events with real-time filters by category, city, and text search, supported by server-side pagination.
- **Tiered Seating & Reservations**: Select ticket quantities across general admission or customized seat tiers (such as VIP, Regular, or Student) with immediate price recalculation.
- **Transactional Booking**: Tickets are reserved inside atomic database transactions with row-locking to prevent overselling popular events.
- **Ticket Passes & Printing**: Access unique alphanumeric ticket passes with dedicated print views stripped of navigation chrome for physical printing or PDF export.
- **Self-Service Cancellation**: Cancel existing reservations prior to event start times, automatically restoring event and tier capacity in real time.

### Organizers
- **Event Lifecycle Control**: Manage events through their full lifecycle: Draft, Published, and Cancelled/Completed.
- **Custom Seat Tier Setup**: Configure independent seat tiers with custom names, seat capacities, and pricing per event.
- **Banner Media Management**: Upload event banners with strict content validation, automatic random filename assignment, and automated cleanup on event deletion.
- **Sales and Revenue Dashboards**: Track aggregate ticket sales, remaining capacity, and revenue generated per event.

### Platform Administrators
- **Platform-Wide Governance**: Review system-wide metrics including total registered users, active organizers, total published events, and gross platform revenue.
- **Event Moderation**: Inspect all events across every organizer with full administrative editing and deletion capabilities.
- **User Auditing**: Monitor account registrations and verify role assignments across the platform.

---

## Architecture Comparison

| Component | Rust Service (`eventpro-rs/`) | Express Service (Root) |
| :--- | :--- | :--- |
| Runtime & Language | Rust 1.80+ / Tokio multi-threaded async runtime | Node.js 18+ / Single-threaded event loop |
| Web Framework | Axum 0.7 (Tower middleware ecosystem) | Express.js 5 |
| HTML Templating | MiniJinja (compiled with automatic contextual HTML escaping) | EJS (Embedded JavaScript) |
| Database Layer | SQLx with asynchronous MySQL connection pooling | `mysql2` connection pool |
| Session Store | `tower-sessions` backed by MySQL (`tower_sessions` table) | `express-session` with in-memory / session store |
| Password Security | Standard `bcrypt` verifying seed blowfish salt rounds | `bcryptjs` |
| CSRF Architecture | Cryptographic tokens validated constant-time on mutative POSTs | Session-based token check |
| File Uploads | Bounded streaming multipart parser with magic-byte verification | `multer` disk storage |
| Static Serving | `tower-http` ServeDir with `X-Content-Type-Options: nosniff` | `express.static` |

---

## Configuration Reference

Configuration is managed via environment variables. Copy `.env.example` to `.env` in the project root or inside `eventpro-rs/`:

```bash
cp .env.example .env
```

| Variable | Requirement | Default | Purpose |
| :--- | :--- | :--- | :--- |
| `DATABASE_URL` | Mandatory (or discrete `DB_*`) | None | MySQL connection string: `mysql://user:pass@host:port/database` |
| `SESSION_SECRET` | Mandatory | None | Secret key used for session signing (minimum 16 characters) |
| `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME` | Alternative fallback | Express values | Discrete connection parameters used when `DATABASE_URL` is unset |
| `PORT` | Optional | `3001` (Rust) / `3000` (Node) | Port on which the HTTP server listens |
| `HOST` | Optional | `127.0.0.1` | Network interface binding |
| `APP_ENV` / `NODE_ENV` | Optional | `development` | Set to `production` to enforce `Secure=true` on session cookies |
| `PUBLIC_DIR` | Optional | `../public` | Directory containing CSS, client JS, and static media |
| `UPLOAD_DIR` | Optional | `PUBLIC_DIR/uploads` | Filesystem location for stored banner image uploads |

*Security Note: The Rust configuration loader implements a custom debug formatter that masks `DATABASE_URL` and `SESSION_SECRET` as `[REDACTED]`. Secrets are never emitted to standard output or log aggregators.*

---

## Getting Started

### 1. Prerequisites
- **MySQL Server 8.0+** running on port `3306`
- **Rust Toolchain 1.80+** (Cargo and rustc)
- **Node.js 18+** and `npm` (required only if running the Express service)

### 2. Database Setup
Initialize the database schema and sample data using your preferred MySQL client:

```bash
# Apply schema tables and constraints
mysql -u root -p < database/schema.sql

# Load initial categories, seed events, seat tiers, and demo accounts
mysql -u root -p event_lifecycle < database/seed.sql
```

The Rust backend automatically creates its session management table (`tower_sessions`) during boot if it does not already exist. The core application tables (`users`, `events`, `categories`, `seat_types`, `bookings`, `tickets`) remain completely unmodified.

### 3. Running the Rust Backend (Recommended)

From the repository root:
```bash
cd eventpro-rs
cargo run
```

The server will compile, connect to MySQL, verify the session store, and listen on `http://localhost:3001` (or your configured `PORT`).

Operational probes:
- **Liveness probe**: `GET http://localhost:3001/health` (returns JSON status)
- **Readiness probe**: `GET http://localhost:3001/ready` (tests live MySQL query execution)

### 4. Running the Legacy Express Backend

If you wish to run the Node.js implementation:
```bash
npm install
npm start
```
The Express server will start on `http://localhost:3000`.

---

## Demonstration Accounts

The seed database includes pre-configured accounts representing each persona:

| Role | Email | Password | Permissions |
| :--- | :--- | :--- | :--- |
| Administrator | `admin@eventpro.com` | `Admin@1234` | Full platform dashboard, event moderation, global analytics |
| Organizer | `bob@events.com` | `Organizer@1` | Event creation and editing, banner uploads, ticket tier setup |
| Attendee | `charlie@mail.com` | `User@1234` | Catalogue browsing, seat selection, booking, tickets, cancellation |

*(Legacy passwords from earlier development seeds: `admin123`, `org123`, `user123` are also supported by the bcrypt hasher).*

## Security and Concurrency Architecture

### 1. CSRF-Protected POST Logout
Session destruction is strictly limited to `POST /auth/logout` and requires a valid CSRF token. Both the top navigation bar and dashboard sidebar render semantic forms with hidden `csrf_token` fields.
- Submitting the logout form with a valid token destroys the session in MySQL and redirects to the public homepage.
- Requests with missing or forged CSRF tokens are rejected with HTTP 403 Forbidden, leaving the user's active session authenticated.
- Requests attempting to access `GET /auth/logout` receive an HTTP 405 Method Not Allowed response, preventing accidental session termination by web crawlers, link pre-fetchers, or browser history navigation.

### 2. Session Fixation Mitigation
Upon every successful user login and account registration, `session.cycle_id()` is invoked immediately before populating user details. This generates a new session key and eliminates session fixation vulnerabilities.

### 3. Concurrency Control and Transaction Integrity
High-demand ticket sales introduce severe race conditions when multiple attendees book the final remaining seats simultaneously:
- **Pessimistic Locking**: The booking handler starts a transaction and issues a `SELECT ... FOR UPDATE` lock on both the event row and the target seat tier row.
- **Strict Capacity Guards**: Capacity checks run inside the locked transaction. If requested tickets exceed available seats, the transaction rolls back cleanly without partial records.
- **Seat Tier Consistency**: If an event defines seat tiers, reservations without a valid tier selection are rejected before allocation.
- **Atomic Cancellation**: Cancelling a booking updates the booking record and restores the event and tier capacity inside a single transactional boundary.
- **Collision Retries**: Unique ticket codes (`EVT-{event_id}-BKG-{booking_id}-{random_hex}`) are checked against uniqueness constraints with automatic retry logic.

### 4. Bounded Multipart Uploads and Filesystem Confinement
Event banner uploads are handled through an isolated streaming parser with hard limits:
- **Request Body Limit**: Event creation and editing endpoints are protected by an 8 MB total request limit (`MAX_EVENT_FORM_BODY_SIZE`), preventing denial-of-service via unbounded requests.
- **File Stream Limit**: Banner files are capped at 5 MB (`MAX_BANNER_SIZE`). Streaming stops and errors cleanly if this limit is passed.
- **Text Field Cap**: Non-file form fields are restricted to 64 KB (`MAX_TEXT_FIELD_SIZE`) to prevent memory exhaustion through oversized textual payloads.
- **Magic-Byte Content Inspection**: Files are inspected for standard magic-byte signatures (JPEG, PNG, WebP, GIF). User-supplied file extensions and MIME headers are untrusted.
- **Safe Filenames**: Uploaded files receive cryptographically random hexadecimal filenames (`event-{timestamp}-{hex}.{ext}`). Path traversal sequences (`..`, `/`, `\`) cannot alter the destination directory.
- **Lifecycle Cleanup**: If a database insert fails after an upload, the orphaned image file is immediately unlinked. When an event is deleted, its dynamic banner is removed while default seed assets (`event-*.jpg`) are safely preserved.
- **Sniffing Protection**: Uploaded files are served with `X-Content-Type-Options: nosniff` headers to stop browsers from executing disguised HTML or script payloads.

### 5. Mock Payment Workflow
The checkout screen (`GET /bookings/payment`) and confirmation endpoint (`POST /bookings/confirm`) simulate an e-commerce checkout. No external payment processors (such as Stripe or PayPal) are invoked. Confirmed bookings immediately issue valid ticket codes for platform demonstration.

## Deployment and Rollback Procedures

### Production Deployment Notes
1. **Reverse Proxy Setup**: In production, deploy EventPro behind Nginx, Caddy, or a cloud load balancer terminating TLS and forwarding traffic to `127.0.0.1:3001`.
2. **Cookie Security**: Set `APP_ENV=production` so the session layer automatically applies `Secure=true` and `SameSite=Lax` to cookies.
3. **Persistent Upload Volume**: In containerized environments (Docker or Kubernetes), map `UPLOAD_DIR` to a persistent shared volume so uploaded media survives pod restarts.
4. **Horizontal Scaling**: The database-backed session table (`tower_sessions`) allows running multiple backend instances behind a round-robin load balancer without requiring sticky sessions.

### Zero-Downtime Rollback
Because the Rust backend introduces zero modifications to existing database tables or columns, rolling back to the Express backend is completely seamless:
1. Re-point your reverse proxy from port `3001` back to port `3000` (Express).
2. The `tower_sessions` table added by Rust will simply be ignored by Node.js.
3. No migration rollbacks, column drops, or data transformations are required.

## Directory Structure

```
.
├── database/                 <- Shared SQL schema and seed data
│   ├── schema.sql            <- Application tables and relational constraints
│   └── seed.sql              <- Categories, demonstration events, and users
├── public/                   <- Shared client assets
│   ├── css/                  <- Application styles
│   ├── js/                   <- Client-side script utilities
│   └── uploads/              <- Uploaded event banners
├── views/                    <- Legacy Node.js Express EJS templates
├── eventpro-rs/              <- Modern Rust / Axum Service
│   ├── Cargo.toml            <- Crate configuration and dependencies
│   ├── src/
│   │   ├── main.rs           <- Server entry point, signal trapping, and listener binding
│   │   ├── lib.rs            <- Root library module definitions
│   │   ├── config.rs         <- Environment loading and credential masking
│   │   ├── csrf.rs           <- CSRF token creation and constant-time validation
│   │   ├── error.rs          <- Centralized error handling and status code mapping
│   │   ├── middleware.rs     <- Role authentication and session guards
│   │   ├── upload.rs         <- Bounded multipart parser and magic-byte checks
│   │   ├── models/           <- Database models (User, Event, Booking, SeatType, Ticket, Category)
│   │   └── routes/           <- Route handlers (events, bookings, auth, tickets, admin, health)
│   ├── templates/            <- MiniJinja server-rendered HTML views
│   └── tests/                <- Test suites (auth, bookings, events, foundation, management, uploads, e2e)
└── README.md                 <- Consolidated project documentation
```

## Testing and Verification Protocol

The Rust codebase features comprehensive test coverage across unit tests, template rendering, and database integration.

To run the verification suite from the `eventpro-rs/` directory:

```bash
cd eventpro-rs

# 1. Format verification
cargo fmt --check

# 2. Strict linter audit
cargo clippy --all-targets -- -D warnings

# 3. Standard test run
cargo test

# 4. Strict real-database MySQL integration suite (97 tests)
# Linux / macOS:
STRICT_DB_TESTS=1 cargo test -j 2

# Windows PowerShell:
& { $env:STRICT_DB_TESTS='1'; cargo test -j 2 }
```

### Strict Database Test Guarantee
When `STRICT_DB_TESTS=1` is specified:
- Tests connect to the live MySQL instance on port 3306.
- If the database is unreachable or misconfigured, the test suite halts with an immediate panic rather than silently skipping integration tests.
- Real transactions, concurrent booking simulation, code collision retries, and banner storage lifecycles are actively exercised against MySQL.

## License
This project uses MIT License. Refer [LICENSE](LICENSE) for more details.