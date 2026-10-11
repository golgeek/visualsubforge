import js from '@eslint/js';
import ts from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';

export default ts.config(
  {
    ignores: [
      'dist/**',
      'playwright-report/**',
      'test-results/**',
      'target/**',
      'src-tauri/target/**',
      'src-tauri/gen/**',
      // Generator-owned runtime helpers; strict TypeScript still checks them.
      'src/lib/generated/**',
      'tests/fixtures/generated/**',
    ],
  },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node, __WINDOWS__: 'readonly' },
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: { parserOptions: { parser: ts.parser } },
  },
);
