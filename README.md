# Axum Template

A single-user, self-hosted web app starter built with Rust, Axum, Askama, HTMX, Tailwind CSS v4, and `libsql`. Comes with authentication, CSRF protection, rate-limited login, persistent sessions, and a password-change page wired up out of the box.

## Features

- **Authentication** — single-password login (argon2id), rate-limited via `tower-governor`
- **CSRF protection** — Synchronizer Token pattern (`axum-tower-sessions-csrf`), injected into every HTMX request
- **Persistent Sessions** — survive server restarts; stored in the `session_store` SQLite table, with periodic expiry cleanup
- **Password change** — inline HTMX form with client- and server-side validation; session id rotation on change
- **Health check** — public `/health` endpoint for load balancers
- **Mobile responsive** — hamburger nav, card layouts, HTMX partial swaps

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (edition 2024) |
| Backend | Axum 0.8 |
| Frontend | Askama templates + HTMX + Tailwind CSS v4.3.2 |
| Database | SQLite via `libsql` (local or Turso remote) |
| Session | `tower-sessions` with custom `SqliteSessionStore` |
| Security | Argon2 password hashing, CSRF (Synchronizer Token), rate limiting |
| Caching | Moka (async in-memory with TTL/TTI) |
| Icons | Font Awesome (CDN) |

## Getting Started

### Prerequisites

- Rust 1.96.0+
- [Taskfile](https://taskfile.dev) (`cargo install task`)
- Tailwind CSS CLI v4 (for CSS builds)
- [prek](https://prek.j178.dev) (`cargo install prek`) — Git hook manager (pre-commit compatible)
- [djlint](https://djlint.org) (`pip install djlint`) — HTML template formatter
- Node.js (for prettier CSS/Dockerfile formatting via prek hooks)

### Setup

```bash
cp .env.example .env
```

Edit `.env` and set at minimum:

```env
APP_PASSWORD_HASH='$argon2id$v=19$m=19456,t=2,p=1$...'  # single quotes required if value contains $
APP_DATABASE_URL=":memory:"  # or "file:data.db" for persistence
```

> **Note:** Values containing `$` (like `APP_PASSWORD_HASH`) must use single quotes in `.env` to prevent variable interpolation.

Generate a password hash:

```bash
# Install argon2 CLI (e.g. brew install argon2 / apt install argon2)
echo -n "yourpassword" | argon2 "$(openssl rand -base64 32)" -e -id -t 2 -m 14 -p 1
```

> **Memory cost matters.** `-m` is the memory exponent: Argon2 uses `2^m` KiB per login attempt. The command above uses `-m 14` (16 MiB), which verifies in ~50 ms. Crank it too high and login stalls — keep `-m` at 14–16 for this single-user app.

### Development

```bash
task dev          # Tailwind watch + cargo watch (hot reload)
task test         # Run all tests
task lint         # Clippy with nursery lints
task fmt          # Format code
task build:css    # Compile Tailwind CSS
```

### Pre-commit Hooks

This project uses [prek](https://prek.j178.dev) to manage Git hooks. Install and activate:

```bash
prek install          # Install Git shims
prek run --all-files  # Run all hooks on tracked files
```

Three formatting hooks run automatically on `git commit`:

| Hook | Tool | Scope |
|------|------|-------|
| Rust formatting | `cargo fmt` | `.rs` files |
| HTML formatting | `djlint --reformat --profile askama` | `.html` templates |
| CSS/Dockerfile formatting | `prettier` | `.css` files, `Dockerfile` |

### Production

```bash
cargo build --release
./target/release/axum-template
```

### Docker

```bash
task docker-build                              # Build image
task docker-run                                # Run on port 8080
task docker-compose-up                         # Run with docker compose
task docker-compose-down                       # Teardown
```

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `APP_PASSWORD_HASH` | Yes | — | Argon2 hash of the single-user password |
| `APP_DATABASE_URL` | No | `:memory:` | SQLite URL (local file, `:memory:`, or Turso remote) |
| `APP_TURSO_TOKEN` | No | `""` | Auth token for Turso remote database |
| `APP_ADDRESS` | No | `0.0.0.0` | Server bind address |
| `APP_PORT` | No | `3000` | Server listen port |
| `RUST_LOG` | No | `info` | Log level filter |

## Architecture

Feature-based modular architecture with layered separation of concerns:

```
src/
├── main.rs              # Bootstrap: env, tracing, db, cache, router, server
├── lib.rs               # Library crate (exposes for integration tests)
├── app.rs               # AppState composition root + create_router
├── config.rs            # Config::from_env() (APP_* vars)
├── db.rs                # Embedded migrations
├── error.rs             # AppError enum with IntoResponse
├── features/            # Feature modules (handlers)
│   ├── auth/            # Login, logout, session middleware, password
│   ├── home/            # Home page + public health check
│   └── settings/        # Password change
├── services/            # Repositories (read + write seams)
│   └── settings.rs      # SettingsRepository (KV store, health check)
└── utils/               # Shared infrastructure
    ├── template.rs      # render_page / render_template
    ├── csrf.rs          # CSRF token helper
    ├── flash.rs         # Session flash messages
    ├── session_store.rs # SqliteSessionStore (tower-sessions)
    ├── cache.rs         # Moka cache helpers
    └── validate.rs      # Parse + validate helper
```

### Layer Responsibilities

| Layer | Responsibilities |
|-------|-----------------|
| **Handlers** | Parse HTTP request, input validation, call repository, render template, CSRF |
| **Services** | SQL queries, cache reads/writes, domain struct construction |
| **Utils** | Pure functions, shared infrastructure, cache primitives |
| **Error** | Unified error type with HTTP response mapping |

### Adding a Feature

1. Create `src/features/<name>/` with a `mod.rs` (declaring `pub mod handlers;`) and `handlers.rs`.
2. Register routes in `src/app.rs` (`create_router`), and import handlers via `crate::features::<name>::handlers`.
3. Put query logic in `src/services/` and reach it through an `AppState` accessor method.
4. Add askama templates under `templates/`.

Handlers never construct services or repositories inline — they get dependencies via the `AppState` composition root (e.g. `state.settings_repo()`).

## Database

2 tables + migrations tracking:

| Table | Purpose |
|-------|---------|
| `settings` | Key-value store (e.g. `password_hash_override`) |
| `session_store` | Persistent session storage |

Migrations are embedded in `src/db.rs` (`include_str!`) and applied at startup inside transactions, tracked in a `_migrations` table. Add new migrations as timestamp-prefixed files under `migrations/` and register them in `db.rs`.

## Testing

```bash
task test
```

Unit tests live inline in each module; integration tests boot the real router over an in-memory database with a cookie- and CSRF-aware `TestClient` (see `tests/`).

## CI

Four GitHub Actions workflows (see `.github/workflows/`):

| Workflow | File | Triggers |
|----------|------|----------|
| CI | `rust-ci.yml` | Push (non-main), PR (opened, synchronize) |
| Audit | `rust-audit.yml` | Weekly schedule, push (non-main), PR |
| Release | `rust-release.yml` | Push to main |
| Build & Push | `rust-push-build.yml` | Push tags (`v*`) |

The Docker image is built with `docker/build-push-action`, tagged with `latest`/semver/sha via `docker/metadata-action`, and pushed to GCR (`IMAGE_NAME: axum-template`).

## License

MIT
