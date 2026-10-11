import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  define: {
    // Tauri's hook environment describes the target, including cross builds.
    __WINDOWS__: JSON.stringify(
      (process.env.TAURI_ENV_PLATFORM ?? process.platform).startsWith('win'),
    ),
  },
  clearScreen: false,
  server: {
    host: 'localhost',
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
});
