import { paraglideVitePlugin } from '@inlang/paraglide-js';
import { sveltekit } from '@sveltejs/kit/vite';
import { playwright } from '@vitest/browser-playwright';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [
    paraglideVitePlugin({
      project: './project.inlang',
      outdir: './src/lib/paraglide',
      strategy: ['localStorage', 'preferredLanguage', 'baseLocale'],
    }),
    sveltekit(),
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: '127.0.0.1',
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**', '**/xtask/**'] },
  },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'logic',
          include: ['tests/**/*.test.ts'],
          exclude: ['tests/**/*.browser.test.ts'],
          environment: 'node',
        },
      },
      {
        extends: true,
        // Scan the dynamic editor before tests start to avoid mid-test dependency reloads.
        optimizeDeps: {
          entries: ['tests/**/*.browser.test.ts', 'src/lib/workbench/editor/Editor.svelte'],
        },
        test: {
          name: 'browser',
          include: ['tests/**/*.browser.test.ts'],
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({ launchOptions: { channel: 'chromium' } }),
            instances: [{ browser: 'chromium' }],
          },
        },
      },
    ],
  },
});
