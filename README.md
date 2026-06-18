# study-engine

A local-first spaced-repetition study tool. A Rust CLI schedules and grades
multiple-choice questions with the [FSRS](https://github.com/open-spaced-repetition)
algorithm, stores your progress in SQLite, and serves a small Svelte web UI for
review. It ships with the Claude Certified Architect Foundations (`cca-f`)
bank as the default, and remains certification-agnostic for any JSON bank you
add.

## Prerequisites

- **Rust** (for the CLI/backend). Install via [rustup](https://rustup.rs):
  - macOS / Linux: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
  - Windows: download and run [`rustup-init.exe`](https://rustup.rs)
- **Node.js** 18+ (for the web UI and the launcher).

## Quick start

The fastest path runs the whole stack with one cross-platform script
(macOS, Linux, and Windows):

```bash
npm install
npm run dev
```

`npm install` installs the Svelte/Vite UI dependencies from the root workspace.
Then `npm run dev` builds the CLI, starts the HTTP API on `:3001`, and serves
the web UI on `:5173`. Open <http://localhost:5173> and start studying the
bundled `cca-f` bank, or go to **Settings** to upload another `<name>.json`
bank. Press Ctrl-C to stop both.

Prefer the terminal? The CLI alone needs no Node:

```bash
cd study-engine-cli
cargo build --release
./target/release/study-engine --cert <name>   # study session
```

## CLI usage

```bash
study-engine                         # study the default cca-f bank
study-engine --cert <name>            # pick another bank
study-engine study --domain 3         # filter to one domain
study-engine study --tag hooks        # filter to one concept tag
study-engine stats                    # progress dashboard
study-engine all                      # quiz every question, shuffled
study-engine serve --port 3001        # HTTP API for the web UI
```

Only three FSRS ratings are used: Again, Good, Easy. Wrong answers are rated
Again automatically; correct answers let you choose Good or Easy by confidence.

## Question banks

A bank is a single JSON file named `<cert>.json`. The repository includes
`questions/cca-f.json` as the default bank for fresh clones. Upload additional
banks through the web UI's Settings tab, or drop them directly into the
questions directory and restart the server. The shape:

```json
{
  "cert": "my-cert",
  "name": "My Certification",
  "domains": {
    "1": "First Domain",
    "2": "Second Domain"
  },
  "questions": [
    {
      "id": "my-cert-001",
      "domain": 1,
      "scenario": "Short scenario label shared across related questions",
      "question": "The prompt the learner answers.",
      "options": { "A": "…", "B": "…", "C": "…", "D": "…" },
      "answer": "A",
      "explanation": "Why A is correct and the others are not.",
      "tags": ["concept-one", "concept-two"]
    }
  ]
}
```

`tags` is optional; everything else is required. `domain` is a number that keys
into the `domains` map. The engine validates the bank on upload — duplicate IDs,
answers not present in options, and unknown domains are all rejected.

The questions directory resolves in this order:

1. `--questions-dir <path>` flag
2. `STUDY_ENGINE_QUESTIONS_DIR` environment variable
3. `questions/` next to the working directory (the development layout)
4. `~/.config/study-engine/questions`

## Where your data lives

Per-card FSRS state and your full review history are stored in a SQLite database
at `~/.local/share/study-engine/study-engine.db`. Deleting that file resets all
progress; the question banks themselves are never modified by the study loop.

## Architecture

- **`study-engine-cli/`** — Rust: CLI study loop, FSRS scheduling, SQLite
  persistence, and an Axum HTTP API (`serve`) that the UI talks to. Also
  generates TypeScript wire types via `ts-rs`.
- **`study-engine-ui/`** — Svelte 5 + Vite frontend. Pure study logic lives in
  `studySessionState.ts`; wire types under `src/lib/generated/` are produced by
  the backend and must not be hand-edited; `api.ts` validates every response
  against Zod schemas at the fetch boundary.
- **`questions/`** — JSON banks. `cca-f.json` is tracked as the bundled default;
  other local banks are ignored unless explicitly added.

`CLAUDE.md` documents modules, data flow, and test conventions in detail. Both
test suites (`cargo test` and `npm run test:coverage`) plus a Playwright e2e
suite run in CI and must pass.

## License

[MIT](LICENSE).
