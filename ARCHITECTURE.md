# Architecture Principle: Declarative Core, Governed Effects

Core software in this project should be declarative and functional by default.
Important system behavior should first exist as explicit models: types, schemas,
state machines, contracts, policies, validation rules, and tests. Runtime
behavior should then be a straightforward consequence of those models.

The goal is not aesthetic purity. The goal is autonomous growth: minimizing
hidden state, side effects, temporal coupling, and implicit behavior so humans
and AI agents can understand, modify, verify, test, and evolve the system safely.

## Rules

1. Keep business logic in pure Rust and TypeScript modules when practical.
2. Treat CLI commands, HTTP handlers, Svelte components, database access, clocks,
   randomness, filesystem access, networking, and user interaction as boundary
   code.
3. Isolate boundary code so it gathers inputs, calls explicit core models, then
   performs observable side effects.
4. Prefer state machines, enums, discriminated unions, and typed contracts over
   loose strings, scattered booleans, or implicit conventions.
5. Give shared rules one home. CLI and API code should not independently
   implement study planning, progress aggregation, card transitions, or review
   validation.
6. Test pure core modules directly. Keep integration tests focused on boundary
   wiring and contract compatibility.

## Current Core Modules

- `study-engine-cli/src/study_plan.rs` owns study-session selection.
- `study-engine-cli/src/progress.rs` owns progress and mastery aggregation.
- `study-engine-cli/src/session.rs` owns review scheduling and card transitions.
- `study-engine-ui/src/lib/studySessionState.ts` owns study-session state updates.
- `study-engine-ui/src/lib/sessionSelectors.ts` owns derived session summaries.
- `study-engine-ui/src/lib/browseSelectors.ts` owns browse filtering and tag derivation.
- `study-engine-ui/src/lib/presentation.ts` owns shared presentation classifiers.

## Boundary Modules

- Rust CLI commands should print, prompt, and call core models.
- Rust HTTP handlers should parse input, load/store data, call core models, and
  serialize responses.
- Svelte components should fetch data, dispatch events, hold local interaction
  state, and render selectors.
