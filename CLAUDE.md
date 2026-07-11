# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**HARetroPanel** — A lightweight Rust-based server-side rendered dashboard for Home Assistant, optimized for old devices (no JavaScript required).

## Quick Commands

```bash
cargo run                          # Run locally (expects .env or env vars)
cargo build --release              # Build release binary
cargo test                         # Run unit tests
cargo clippy                       # Lint
cargo fmt                          # Format code
make build                         # Full build (npm + cargo)
make js-check                      # Transpile JS and validate ES2019
```

## Configuration

All config is via environment variables (`.env` file or shell). See `config/app_config.rs`.

Required: `HA_BASE_URL`, `HA_TOKEN`
Optional defaults: port `8080`, log rotation `daily`, cache TTL `5s`

## Architecture

Clean architecture with four layers, dependency rule: infra → application → domain (domain has zero deps).

**Domain** (`src/domain/`) — Entities (`Entity`, `DashboardState`, `EntityKind`) and value objects (`EntityId`). No external deps.

**Application** (`src/application/`) — Use cases via `DashboardService`. Two port traits:
- `HomeAssistantClient` (port: how to talk to HA API)
- `DashboardLayoutRepository` (port: how to persist visible entities / page assignments)

Service has an in-memory cache with per-entity-kind TTL. Writes (toggle/run_script) invalidate cache.

**Infrastructure** (`src/infrastructure/`) — Implementations of ports:
- `ha::HaHttpClient` — Home Assistant REST API (GET /api/states, POST /api/services/*)
- `layout::FsDashboardLayoutRepository` — JSON file on disk (`./data/dashboard_layout.json`)
- `web` — Axum router, handlers, Askama templates, view models

**Bootstrap** (`bootstrap.rs`) — Wire everything together, build Axum app, bind TCP listener.

## Web Routes

- `GET /` — Dashboard page (query: `?page=N&force_refresh=1`)
- `POST /toggle` — Toggle light/switch (form: `entity_id`)
- `POST /run_script` — Run a script entity (form: `entity_id`)
- `GET /settings/entities` — Entity visibility & page assignment settings
- `POST /settings/entities` — Save settings
- `GET /ws/solar` — WebSocket for real-time solar data
- `GET /js/*` — Served transpiled JS from `public/js/`

Templates live in `templates/` (Askama). View models in `infrastructure/web/viewmodels.rs`.

## Key Patterns

- `AppResult<T>` / `AppError` — thiserror enum with Http, Template, Config, Internal variants
- `AppState` — Axum state container holding `Arc<DashboardService>`
- Traits are defined in application layer, implemented in infrastructure (port/adaptor pattern)
- Layout JSON supports legacy formats via fallback deserialization in `FsDashboardLayoutRepository::load_raw`

## Frontend JavaScript

All JS is in `src/js/`, transpiled to ES2019 (Safari 12) via Babel during build. Output goes to `public/js/`.

Templates inject server data via `window.__HARetro__` before loading external scripts:

```html
<script>
    window.__HARetro__ = {
        chartLabels: {{ dashboard.solar.chart_labels_js | safe }},
        chartValues: {{ dashboard.solar.chart_values_js | safe }},
        // ... more data
    };
</script>
<script src="/js/dashboard.js"></script>
```

When modifying template JavaScript, always edit `src/js/` files — never put inline `<script>` blocks.
Use `make js-check` to verify ES2019 compliance (uses `es-check`).

