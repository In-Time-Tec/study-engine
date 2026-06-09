import { defineConfig } from 'vite'
import { svelte, vitePreprocess } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte({ preprocess: vitePreprocess() })],
  server: {
    // Defaults suit `npm run dev`; the e2e suite overrides these so it never
    // collides with a running dev stack (boot.sh) on the standard ports.
    port: Number(process.env.VITE_PORT) || 5173,
    proxy: {
      '/api': process.env.VITE_API_PROXY || 'http://localhost:3001'
    }
  }
})
