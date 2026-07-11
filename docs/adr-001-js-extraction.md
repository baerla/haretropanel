# ADR-001: Extract Inline JavaScript to Separate Transpiled Files

## Status

Accepted

## Date

2025-07-11

## Context

HARetroPanel renders server-side templates with inline `<script>` blocks containing JavaScript. When `??` (nullish coalescing) was used in dashboard.html line 668, the page crashed on a 2014 iPad running Safari 12, which only supports ES2019. The nullish coalescing operator `??` was introduced in ES2020.

Replacing `??` with `!= null` was a band-aid fix — there was no systematic way to prevent future ES2020+ features from creeping in, and inline scripts are hard to test, lint, or transpile.

## Decision

- Extract all inline JavaScript from templates into separate `.js` files under `src/js/`
- Add a Babel build step that transpiles `src/js/*.js` → `public/js/*.js` targeting Safari 12 (ES2019)
- Use `es-check` to validate ES2019 compliance in CI / before deploy
- Templates receive server data via `window.__HARetro__` data block, then load external scripts via `<script src="/js/...">`
- Serve transpiled JS via `tower-http` `ServeDir` on `/js/*`

## Consequences

### Positive
- Guaranteed ES2019 compliance via automated checks (`es-check`)
- JS files are lintable, testable, and diffable independently
- Server data injection pattern is explicit and consistent (`window.__HARetro__`)
- Templates are cleaner and more maintainable
- Old devices (Safari 12, Chrome 76) are supported

### Negative
- Build pipeline is now Rust + Node.js
- Dockerfile requires Node.js for JS transpilation in the build stage
- Static file serving adds a small runtime dependency (`tower-http` `fs` feature)
- Two build steps (`npm run build` + `cargo build`) instead of one

### Trade-offs considered
- **Inline `{{ }}` templates vs `window.__HARetro__`** — Inline templates are simpler but require a different transpile target per feature branch. The `window.__HARetro__` pattern decouples data injection from code, enabling consistent ES2019 targeting.
- **TypeScript vs Babel** — TypeScript would add type safety but introduces compilation overhead and complexity for a codebase of this size. Babel + `es-check` is lighter weight.
- **Vite/Rollup vs Babel CLI** — More sophisticated bundlers would handle tree-shaking, but the project has a single JS file with no dependencies to bundle. Babel CLI is sufficient.
