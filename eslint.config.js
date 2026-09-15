import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import svelte from 'eslint-plugin-svelte';
import { defineConfig, globalIgnores } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';

import svelteConfig from './svelte.config.ts';

const projectService = { allowDefaultProject: ['svelte.config.ts', 'stylelint.config.ts'] };

export default defineConfig(
  globalIgnores([
    '.svelte-kit/**',
    'build/**',
    '**/target/**',
    'src-tauri/gen/**',
    'src/lib/paraglide/**',
    'src/lib/protocol/generated/**',
    'test-results/**',
  ]),
  { files: ['**/*.{js,mjs,ts,svelte}'], extends: [js.configs.recommended] },
  {
    files: ['src/**/*.{ts,svelte}', 'tests/**/*.ts', '*.config.ts'],
    extends: [ts.configs.strictTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService,
        extraFileExtensions: ['.svelte'],
      },
    },
    rules: { '@typescript-eslint/switch-exhaustiveness-check': 'error' },
  },
  ...svelte.configs.recommended,
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        parser: ts.parser,
        projectService,
        extraFileExtensions: ['.svelte'],
        svelteConfig,
      },
    },
  },
  {
    files: ['src/**/*.{ts,svelte}'],
    ignores: ['src/lib/desktop/**'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            { group: ['@tauri-apps/*'], message: 'Access Tauri through the desktop boundary.' },
          ],
        },
      ],
    },
  },
  {
    files: ['**/*.{js,mjs}'],
    languageOptions: { globals: { console: 'readonly', process: 'readonly', URL: 'readonly' } },
  },
  prettier,
);
