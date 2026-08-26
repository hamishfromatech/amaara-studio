import js from "@eslint/js";
import react from "eslint-plugin-react";

export default [
  { ignores: ["dist", "node_modules"] },
  js.configs.recommended,
  {
    ...react.configs.flat.recommended,
    settings: { react: { version: "detect" } },
  },
];
