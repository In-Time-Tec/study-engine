import { defineConfig } from 'vitest/config'
import { svelte, vitePreprocess } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte({ preprocess: vitePreprocess(), compilerOptions: { dev: false } })],
  resolve: {
    conditions: ['browser']
  },
  test: {
    environment: 'jsdom',
    // Unit/component tests live under src/. The Playwright e2e suite under e2e/
    // runs separately (`npm run e2e`) and must not be picked up by vitest.
    include: ['src/**/*.{test,spec}.ts'],
    setupFiles: ['./src/test/setup.ts'],
    coverage: {
      provider: 'istanbul',
      all: true,
      // Humble View: all decisions and transformations live in these pure .ts
      // modules, gated at a flat 100%. The .svelte components are a thin,
      // declarative view layer — their behaviour is verified by the component
      // tests in presentation.test.ts, but they are intentionally NOT coverage-
      // gated. A non-trivial .svelte file cannot honestly reach 100%: Svelte 5
      // emits internal runtime branches that are dead under jsdom conditions,
      // so gating on them only ratchets the threshold down to the compiler's
      // noise floor. Keeping the
      // gate on logic-only modules makes 100% both achievable and meaningful.
      // Excluded by design: *.svelte (view), types.ts (type-only declarations),
      // main.ts (bootstrap glue).
      include: [
        'src/lib/api.ts',
        'src/lib/browseSelectors.ts',
        'src/lib/dashboardHelp.ts',
        'src/lib/presentation.ts',
        'src/lib/sessionSelectors.ts',
        'src/lib/studySessionState.ts',
        'src/lib/theme.ts'
      ],
      reporter: ['text', 'json-summary'],
      thresholds: {
        statements: 100,
        branches: 100,
        functions: 100,
        lines: 100
      }
    }
  }
})
