import js from "@eslint/js";

const nodeGlobals = {
  process: "readonly",
  console: "readonly",
  __dirname: "readonly",
  Buffer: "readonly",
  setTimeout: "readonly",
  clearTimeout: "readonly",
};

// The codebase marks deliberately-unused bindings with a leading underscore.
const unusedVars = [
  "error",
  {
    argsIgnorePattern: "^_",
    varsIgnorePattern: "^_",
    caughtErrorsIgnorePattern: "^_",
  },
];

const browserGlobals = {
  window: "readonly",
  document: "readonly",
  console: "readonly",
  navigator: "readonly",
  location: "readonly",
  fetch: "readonly",
  setTimeout: "readonly",
  clearTimeout: "readonly",
  setInterval: "readonly",
  clearInterval: "readonly",
  requestAnimationFrame: "readonly",
  CustomEvent: "readonly",
  Event: "readonly",
  MutationObserver: "readonly",
  URL: "readonly",
  getComputedStyle: "readonly",
  HTMLElement: "readonly",
  customElements: "readonly",
};

export default [
  {
    ignores: [
      "node_modules/**",
      "src-tauri/**",
      "turbo_desktop-rails/**",
      "site/**",
      "debug_project/**",
      "test_raider_project/**",
      "turbo_desktop_example_app/**",
      "graphify-out/**",
    ],
  },
  js.configs.recommended,
  { rules: { "no-unused-vars": unusedVars } },
  {
    // The CLI, the config itself and the test suite run in Node.
    files: ["cli/**/*.js", "test/**/*.js", "eslint.config.js"],
    languageOptions: {
      ecmaVersion: 2023,
      sourceType: "module",
      globals: nodeGlobals,
    },
  },
  {
    // src/ and packages/ are injected into a browser page.
    files: ["src/**/*.js", "packages/**/*.js"],
    languageOptions: {
      ecmaVersion: 2023,
      sourceType: "module",
      globals: browserGlobals,
    },
  },
  {
    // The e2e suite is a Node process whose execute() callbacks run in the
    // app's webview, so it legitimately uses both worlds' globals.
    files: ["e2e/**/*.mjs"],
    languageOptions: {
      ecmaVersion: 2023,
      sourceType: "module",
      globals: { ...nodeGlobals, ...browserGlobals },
    },
  },
];
