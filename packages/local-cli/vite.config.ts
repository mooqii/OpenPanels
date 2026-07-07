import { builtinModules } from "node:module"
import { resolve } from "node:path"
import { defineConfig } from "vite"

export default defineConfig({
  build: {
    emptyOutDir: true,
    minify: false,
    outDir: "dist",
    rollupOptions: {
      external: [
        ...builtinModules,
        ...builtinModules.map((moduleName) => `node:${moduleName}`),
      ],
      input: resolve(import.meta.dirname, "src/index.ts"),
      output: {
        banner: "#!/usr/bin/env node",
        entryFileNames: "openpanels-local.mjs",
        format: "es",
      },
    },
    ssr: true,
    target: "node20",
  },
})
