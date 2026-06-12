# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Projects

study-engine is a local-first spaced-repetition study tool. It ships with the Claude Certified Architect Foundations (`cca-f`) bank as the default while remaining certification-agnostic for additional `<cert>.json` banks. Two codebases live here side by side, plus the questions directory:

- **`study-engine-cli/`** — Rust backend: CLI study tool + Axum HTTP API server
- **`study-engine-ui/`** — Svelte 4 + Vite frontend
- **`questions/`** — JSON question banks (`cca-f.json` is tracked as the bundled default; other local banks are ignored unless explicitly added)

## Commands

### Full dev stack
```bash
node boot.mjs      # cross-platform (macOS/Linux/Windows): builds CLI, starts backend on :3001 and UI on :5173
```

### Backend (study-engine-cli/)
```bash
cargo build                          # debug build
cargo build --release                # release build
cargo test                           # all tests (also regenerates src/lib/generated/)
cargo test <test_name>               # single test by name
cargo test -- --nocapture            # show println! output
RUST_LOG=debug cargo run -- study    # run with trace logging
```

### Frontend (study-engine-ui/)
```bash
npm run dev            # dev server on :5173 (proxies /api → :3001)
npm test               # vitest run (headless)
npm run test:coverage  # coverage report — enforces thresholds (see below)
npm run typecheck      # tsc check that Zod schemas satisfy generated wire types
npm run e2e            # Playwright suite (requires built backend binary)
```

### CLI usage
```bash
study-engine                              # study the default cca-f bank
study-engine --cert <name>                # pick another bank
study-engine study --domain 3             # filter to domain 3
study-engine stats                        # progress dashboard
study-engine all                          # quiz every question, shuffled
study-engine serve --port 3001            # HTTP API for the web UI
```

Questions dir resolves in order: `--questions-dir` flag → `STUDY_ENGINE_QUESTIONS_DIR` env → `questions/` sibling → `~/.config/study-engine/questions`.

## Architecture

### Data flow
1. `Bank::load()` reads `<cert>.json` from the questions dir into memory. `Bank::parse()` is the shared validation path used by both file loads and the upload endpoint.
2. `Db` (SQLite at `~/.local/share/study-engine/study-engine.db`) stores per-card FSRS state (`cards` table) and raw review history (`reviews` table).
3. On each review, `fsrs_next()` in `session.rs` computes new stability/difficulty/due from the FSRS algorithm and `Db::record_review()` writes both tables atomically. Both `fsrs_next` and `days_since_review` accept an injected `today: NaiveDate` rather than reading the clock internally, keeping scheduling a pure function.

### FSRS ratings
Only three ratings are used — there is no "Hard" (2):
- `1` = Again (wrong or want to repeat)
- `3` = Good (correct, unsure)
- `4` = Easy (correct, confident)

The CLI auto-assigns `1` for wrong answers without prompting. The web UI gates "Good" and "Easy" behind `isCorrect`.

### Backend modules
| File | Role |
|---|---|
| `main.rs` | CLI parsing (clap), questions dir resolution, subcommand dispatch |
| `questions.rs` | `Bank` and `Question` types; `Bank::parse` validates raw JSON (shared by load and upload) |
| `db.rs` | `Db` wrapper over rusqlite; `CardState` with FSRS fields |
| `session.rs` | CLI study loop + `fsrs_next()` (shared with serve) |
| `serve.rs` | Axum HTTP handlers; bank CRUD (`GET/POST /api/banks`, `DELETE /api/banks/{cert}`); session resume (`GET/POST/DELETE /api/pending-session`); generates TypeScript wire types via `ts-rs` |
| `progress.rs` | Domain/tag/session aggregation shared between CLI stats and the API |
| `stats.rs` | CLI progress dashboard with ANSI bar charts |

### TypeScript wire types

Backend response structs derive `ts_rs::TS`. The `serve::ts_bindings::export_typescript_bindings` cargo test writes them to `study-engine-ui/src/lib/generated/`. **Do not hand-edit files in that directory** — run `cargo test` to regenerate. CI fails if the committed output drifts from the Rust structs.

`study-engine-ui/src/lib/types.ts` re-exports the generated types under the names the app uses. `src/lib/schemas.ts` mirrors them as Zod schemas; `api.ts` parses every fetch response through the relevant schema at the boundary.

### Frontend modules
| File | Role |
|---|---|
| `App.svelte` | Tab shell; owns cert selection and `applyCerts` for stable selection across refreshes |
| `StudySession.svelte` | Study state machine (`loading → question → revealed → summary`); owns in-session mode toggle, domain filter, and session resume via the DB (`/api/pending-session`) |
| `Dashboard.svelte` | Stats overview; clickable domain bars deep-link into due sessions; collapsible help panel |
| `Settings.svelte` | Bank upload (with 409-conflict confirmation), bank delete, cert switch |
| `api.ts` | Fetch wrapper; Zod-validates every response; handles 409-as-conflict for upload |
| `studySessionState.ts` | Pure study logic — card selection, rating, result accumulation |
| `schemas.ts` | Zod schemas mirroring generated wire types; `satisfies z.ZodType<T>` pins enforce sync with TypeScript |
| `theme.ts` | Theme presets (amber/dark/light), token derivation, localStorage persistence |
| `dashboardHelp.ts` | Collapsed-state persistence for the Dashboard help panel |
| `generated/` | Auto-generated TypeScript types from Rust structs — do not edit |

### Tests and coverage

Both test suites must pass before any change is considered done — they are independent and neither implies the other.

**Backend** (`study-engine-cli/`): `cargo test`
- Interactive I/O functions are excluded via `#[cfg(not(tarpaulin_include))]` so they don't drag down coverage metrics.
- All DB tests use `Db::open_in_memory()` — no on-disk state.

**Frontend** (`study-engine-ui/`): `npm test` / `npm run test:coverage`
- `studySessionState.ts` must maintain **100% coverage** (statements, branches, functions, lines) — enforced as a hard threshold in `vitest.config.mjs`.
- Global thresholds: ≥90% statements, ≥60% branches, ≥93% functions, ≥99% lines. The branch floor is lower because Svelte 4's compiler generates internal reactive branches that Istanbul counts but that don't map to authored code.
- Coverage is tracked for: `api.ts`, `browseSelectors.ts`, `dashboardHelp.ts`, `presentation.ts`, `sessionSelectors.ts`, `studySessionState.ts`, `theme.ts`.
- When adding logic to any covered file, tests must cover every new branch.
- Playwright specs live in `e2e/` and run separately via `npm run e2e` — vitest excludes that directory.

**E2e** (`study-engine-ui/`): `npm run e2e`
- Requires a built backend binary (`cargo build` from `study-engine-cli/`).
- Uses env vars `VITE_PORT` and `VITE_API_PROXY` to avoid colliding with a running dev stack.
- Two specs: `study-flow.spec.ts` (card load, answer, summary) and `settings-flow.spec.ts` (bank upload, cert switch, delete).
