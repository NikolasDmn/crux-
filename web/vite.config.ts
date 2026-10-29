/// <reference types="vitest/config" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

// Dev: `npm run dev` proxies the API to the gateway (the compose stack), so cookies and SSE work
// same-origin, as in production where the gateway serves `dist/` (frontend-spec 0).
const gateway = process.env.CRUX_GATEWAY ?? "http://localhost:8000";

// pdf.js draws non-embedded fonts (Helvetica, Times…) from its standard font data, and CJK text
// needs its character maps. With font faces off (CSP), a PDF without them renders blank. Served at
// /pdfjs/ in dev and emitted into dist/pdfjs/ by the build.
function pdfjsAssets(): Plugin {
  const root = fileURLToPath(new URL("./node_modules/pdfjs-dist/", import.meta.url));
  const dirs = ["standard_fonts", "cmaps"];
  return {
    name: "crux-pdfjs-assets",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const m = /^\/pdfjs\/(standard_fonts|cmaps)\/([\w.-]+)$/.exec(req.url ?? "");
        if (!m) return next();
        try {
          res.end(readFileSync(join(root, m[1]!, m[2]!)));
        } catch {
          next();
        }
      });
    },
    generateBundle() {
      for (const dir of dirs) {
        for (const name of readdirSync(join(root, dir))) {
          if (name.startsWith("LICENSE")) continue;
          this.emitFile({
            type: "asset",
            fileName: `pdfjs/${dir}/${name}`,
            source: readFileSync(join(root, dir, name)),
          });
        }
      }
    },
  };
}

// Spec 6: Home on a cold load. The shell and Home route chunks are only found once the router
// runs; preload them from index.html so they load alongside the entry, not after it.
function preloadFirstScreen(): Plugin {
  const routes = [
    /src\/routes\/_app\.tsx\?tsr-split=component$/,
    /src\/routes\/_app\/index\.tsx\?tsr-split=component$/,
  ];
  return {
    name: "crux-preload-first-screen",
    apply: "build",
    transformIndexHtml: {
      order: "post",
      handler(_html, ctx) {
        const bundle = ctx.bundle;
        if (!bundle) return [];
        const chunks = Object.values(bundle).filter((c) => c.type === "chunk");
        // Just the two route chunks: preloading their imports too (40-odd small files) and the font
        // competed with the stylesheet and made the first paint later, not sooner.
        const files = chunks
          .filter((c) => c.facadeModuleId && routes.some((r) => r.test(c.facadeModuleId!)))
          .map((c) => c.fileName);
        // The text font too: otherwise it's found only once the stylesheet applies, and the largest
        // text (Home's heading) repaints when it finally arrives.
        const font = Object.values(bundle).find((a) => /geist-latin-wght-normal.*\.woff2$/.test(a.fileName));
        return [
          ...files.map((f) => ({
            tag: "link",
            attrs: { rel: "modulepreload", href: `/${f}` },
            injectTo: "head" as const,
          })),
          ...(font
            ? [
                {
                  tag: "link",
                  attrs: { rel: "preload", as: "font", type: "font/woff2", href: `/${font.fileName}`, crossorigin: "" },
                  injectTo: "head" as const,
                },
              ]
            : []),
        ];
      },
    },
  };
}

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
    pdfjsAssets(),
    preloadFirstScreen(),
  ],
  resolve: {
    alias: [
      { find: "@", replacement: fileURLToPath(new URL("./src", import.meta.url)) },
      // MathJax's default SVG font is replaced by the smaller TeX font (src/lib/mathjax-font.ts).
      {
        find: /^#default-font\/svg\/default\.js$/,
        replacement: fileURLToPath(new URL("./src/lib/mathjax-font.ts", import.meta.url)),
      },
    ],
  },
  server: {
    port: 5173,
    proxy: {
      "/api": { target: gateway, changeOrigin: false },
      // Signed for the gateway's host: forward with its Host header so the signature matches.
      "/crux-files": { target: gateway, changeOrigin: true },
    },
  },
  // No data: URLs: the CSP allows fonts and images from 'self' only.
  build: {
    outDir: "dist",
    sourcemap: true,
    target: "es2022",
    assetsInlineLimit: 0,
    manifest: true,
  },
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
