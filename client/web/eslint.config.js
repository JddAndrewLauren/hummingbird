import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import jsxA11y from "eslint-plugin-jsx-a11y";
import globals from "globals";

// The repo had six `// eslint-disable-next-line react-hooks/exhaustive-deps`
// comments and no ESLint config at all, so nothing was checking those
// dependency arrays — the suppressions read as "someone considered this" while
// meaning nothing. This config makes them real.
//
// jsx-a11y is here because the accessibility defects in the component layer
// (see HANDOFF-a11y-designsystem.md) are exactly the class a linter catches
// for free. Its rules start as warnings so the existing, known violations do
// not block the build before that handoff is worked; `--max-warnings 0` is the
// goal once they are fixed.
export default tseslint.config(
  {
    ignores: ["dist/**", "dev-dist/**", "src/wasm/pkg/**", "node_modules/**"],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      globals: { ...globals.browser, ...globals.es2024 },
      parserOptions: { ecmaFeatures: { jsx: true } },
    },
    plugins: {
      "react-hooks": reactHooks,
      "jsx-a11y": jsxA11y,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      ...jsxA11y.flatConfigs.recommended.rules,
      // The point of adding ESLint: this must be an error, or the six
      // suppressions in useCalendarWiring.ts stay decorative.
      "react-hooks/exhaustive-deps": "error",
      // Known, tracked violations in the ported design-system components.
      // Warnings until HANDOFF-a11y-designsystem.md is worked.
      "jsx-a11y/no-static-element-interactions": "warn",
      "jsx-a11y/click-events-have-key-events": "warn",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    files: ["**/*.test.ts"],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    files: ["csp-worker/**/*.ts", "*.config.ts", "*.config.js"],
    languageOptions: { globals: { ...globals.node } },
  },
);
