import { defineConfig } from '@playwright/test'

// Non-standard ports keep the e2e isolated from a running dev stack (boot.sh
// uses 3001/5173), so reuseExistingServer can't grab the wrong backend.
const API_PORT = 3101
const UI_PORT = 5273

// End-to-end config. Two servers come up for the run:
//   1. the Rust API (`study-engine serve`) against a throwaway HOME so its
//      SQLite DB starts empty, pointed at the committed e2e fixture bank;
//   2. the Vite dev server, which proxies /api → :3001.
// The backend binary is built with the normal environment (so cargo finds its
// toolchain/registry under the real HOME); only the *running* binary gets an
// isolated HOME, which is all that controls the DB path.
export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: `http://localhost:${UI_PORT}`,
    trace: 'on-first-retry'
  },
  // The backend binary is built with the normal environment (so cargo finds its
  // toolchain under the real HOME); only the running binary gets an isolated
  // HOME, which is all that controls the SQLite DB path. Vite is told to proxy
  // /api to the e2e backend.
  webServer: [
    {
      command: `bash -c 'cargo build --manifest-path ../study-engine-cli/Cargo.toml && HOME="$(mktemp -d)" STUDY_ENGINE_QUESTIONS_DIR="$PWD/e2e/fixtures" ../study-engine-cli/target/debug/study-engine serve --port ${API_PORT}'`,
      port: API_PORT,
      timeout: 240_000,
      reuseExistingServer: !process.env.CI
    },
    {
      command: `VITE_PORT=${UI_PORT} VITE_API_PROXY=http://localhost:${API_PORT} npm run dev`,
      port: UI_PORT,
      timeout: 120_000,
      reuseExistingServer: !process.env.CI
    }
  ]
})
