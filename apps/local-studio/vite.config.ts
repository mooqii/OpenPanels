import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

const apiProxyTarget =
  process.env.OPENPANELS_API_BASE ?? process.env.OPENPANELS_STUDIO_API_BASE

export default defineConfig({
  server: {
    cors: true,
    headers: {
      "Access-Control-Allow-Headers": "content-type",
      "Access-Control-Allow-Methods": "GET,POST,PUT,PATCH,DELETE,OPTIONS",
      "Access-Control-Allow-Origin": "*",
    },
    proxy: apiProxyTarget
      ? {
          "/api": {
            changeOrigin: true,
            target: apiProxyTarget,
          },
        }
      : undefined,
  },
  plugins: [tailwindcss(), react()],
  resolve: {
    alias: {
      "~/canvas": path.resolve(import.meta.dirname, "src/canvas"),
      "@lingui/react/macro": path.resolve(
        import.meta.dirname,
        "src/canvas/i18n-shim.tsx"
      ),
    },
  },
})
