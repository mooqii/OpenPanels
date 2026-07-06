import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"
import { createOpenPanelsApiMiddleware } from "../../packages/local-server/src/index"

export default defineConfig({
  plugins: [
    {
      name: "openpanels-local-api",
      configureServer(server) {
        server.middlewares.use(
          createOpenPanelsApiMiddleware(
            process.env.OPENPANELS_PROJECT_DIR ??
              path.resolve(import.meta.dirname, "../..")
          )
        )
      },
    },
    tailwindcss(),
    react(),
  ],
  resolve: {
    alias: {
      "~/canvas": path.resolve(
        import.meta.dirname,
        "../../packages/canvas/src"
      ),
      "@lingui/react/macro": path.resolve(
        import.meta.dirname,
        "../../packages/canvas/src/i18n-shim.tsx"
      ),
    },
  },
})
