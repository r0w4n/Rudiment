import js from '@eslint/js';
import globals from 'globals';

const sharedRules = {
  'no-unused-vars': ['error', { argsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' }],
};

export default [
  {
    ignores: [
      'dist/**',
      'dist-pages/**',
      'node_modules/**',
      'performance-results/**',
      'quality-results/**',
      'spikes/alphatab-dist/**',
      'target/**',
      'test-results/**',
      'web/public/bridge.js',
      'web/dist/**',
    ],
  },
  {
    files: ['**/*.{js,mjs}'],
    ...js.configs.recommended,
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: { ...globals.browser, ...globals.node },
    },
    rules: sharedRules,
  },
];
