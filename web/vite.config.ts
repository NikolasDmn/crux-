/// <reference types="vitest/config" />
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Dev: `npm run dev` proxies the API to the gateway (the compose stack), so cookies and SSE work
// same-origin, as in production where the gateway serves `dist/` (frontend-spec 0).
const gateway = process.env.CRUX_GATEWAY ?? "http://localhost:8000";

export default defineConfig({
  plugins: [
    tanstackRouter({
      target: "react",
      routesDirectory: "src/routes",
      generatedRouteTree: "src/routeTree.gen.ts",
      autoCodeSplitting: true,
    }),
    react(),
    tailwindcss(),
  ],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  server: {
    port: 5173,
    proxy: {
      "/api": { target: gateway, changeOrigin: false },
      "/crux-files": { target: gateway, changeOrigin: false },
    },
  },
  build: { outDir: "dist", sourcemap: true, target: "es2022" },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    coverage: {
      provider: "v8",
      include: ["src/lib/**", "src/features/*/lib/**"],
      exclude: ["src/lib/api.gen.ts", "**/*.test.*"],
      thresholds: { lines: 80 },
    },
  },
});
