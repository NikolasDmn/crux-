import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "coverage", "src/routeTree.gen.ts", "src/lib/api.gen.ts"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ["**/*.{ts,tsx}"],
    languageOptions: { ecmaVersion: 2022, globals: globals.browser },
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
      // Components never call fetch: the hooks in features/*/api.ts do (frontend-spec 1).
      "no-restricted-globals": ["error", { name: "fetch", message: "Use the API client in src/lib/api.ts." }],
    },
  },
  {
    files: ["src/lib/**/*.ts"],
    rules: { "no-restricted-globals": "off" },
  },
);
