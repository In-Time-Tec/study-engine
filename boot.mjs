#!/usr/bin/env node
// Cross-platform dev launcher (macOS / Linux / Windows).
// Builds the Rust backend, then runs the API server and Vite UI together.
// Replaces boot.sh; run with `node boot.mjs`.

import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const isWin = process.platform === "win32";

// npm ships as npm.cmd on Windows; cargo/the built binary keep their names.
const npm = isWin ? "npm.cmd" : "npm";
const binName = isWin ? "study-engine.exe" : "study-engine";
const backendBin = join(SCRIPT_DIR, "study-engine-cli", "target", "debug", binName);
const uiDir = join(SCRIPT_DIR, "study-engine-ui");
const manifest = join(SCRIPT_DIR, "study-engine-cli", "Cargo.toml");

// Build the backend first so compile errors surface before anything starts.
console.log("Building study-engine-cli...");
const build = spawnSync("cargo", ["build", "--manifest-path", manifest], {
  stdio: "inherit",
  shell: isWin, // resolve cargo.exe via PATHEXT on Windows
});
if (build.status !== 0) {
  console.error("Build failed.");
  process.exit(build.status ?? 1);
}

const children = [];
let shuttingDown = false;

function start(label, command, args, opts = {}) {
  const child = spawn(command, args, { stdio: "inherit", shell: isWin, ...opts });
  children.push(child);
  child.on("exit", (code) => {
    if (!shuttingDown) {
      console.log(`${label} exited (code ${code}); stopping the rest.`);
      cleanup();
    }
  });
  return child;
}

function cleanup() {
  if (shuttingDown) return;
  shuttingDown = true;
  for (const child of children) {
    child.kill();
  }
  console.log("Stopped.");
  process.exit(0);
}

process.on("SIGINT", cleanup);
process.on("SIGTERM", cleanup);

console.log("Starting backend on :3001...");
start("backend", backendBin, ["serve", "--port", "3001"]);

console.log("Starting UI on :5173...");
start("UI", npm, ["run", "dev"], { cwd: uiDir });

console.log("");
console.log("  UI  → http://localhost:5173");
console.log("  API → http://localhost:3001");
console.log("");
console.log("Ctrl-C to stop both.");
