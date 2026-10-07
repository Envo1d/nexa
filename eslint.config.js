import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import vue from "eslint-plugin-vue";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/**", "dist-ssr/**", "src-tauri/**", "node_modules/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs["flat/recommended"],
  {
    files: ["src/**/*.{ts,vue}"],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ["**/*.vue"],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
  },
  {
    files: ["*.{js,ts}"],
    languageOptions: { globals: globals.node },
  },
  prettier,
);
