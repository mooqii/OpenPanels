import { readFileSync } from "node:fs"
import { createRequire } from "node:module"

import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
} from "@modelcontextprotocol/ext-apps/server"

const require = createRequire(import.meta.url)
const EXPORT_BLOCK_REGEX = /^export\{([^}]+)\};?\s*$/s
const EXPORT_ALIAS_REGEX = /\s+as\s+/
let cachedMcpAppsGlobalScript = ""

export function registerWidgetResource(
  server,
  {
    name,
    uri,
    title,
    description,
    html,
    prefersBorder = false,
    connectDomains = [],
    resourceDomains = [],
    frameDomains = [],
  }
) {
  const metadata = {
    ui: {
      prefersBorder,
      csp: {
        connectDomains,
        resourceDomains,
        frameDomains,
      },
    },
    "openai/widgetDescription": description,
    "openai/widgetPrefersBorder": prefersBorder,
    "openai/widgetCSP": {
      connect_domains: connectDomains,
      resource_domains: resourceDomains,
      frame_domains: frameDomains,
    },
  }

  registerAppResource(
    server,
    name,
    uri,
    {
      title,
      description,
      _meta: metadata,
    },
    async () => ({
      contents: [
        {
          uri,
          mimeType: RESOURCE_MIME_TYPE,
          text: typeof html === "function" ? await html() : html,
          _meta: metadata,
        },
      ],
    })
  )
}

export function openPanelsWidgetHtml({
  appHtml,
  initialApiBase,
  initialBootstrap,
  initialProjectDir,
  initialStorageDir,
  initialDisplayMode = "fullscreen",
} = {}) {
  return injectMcpHostBridge(appHtml ?? fallbackWidgetHtml(), {
    initialApiBase,
    initialBootstrap,
    initialDisplayMode,
    initialProjectDir,
    initialStorageDir,
  })
}

function fallbackWidgetHtml() {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>MyOpenPanels</title>
    <style>
      html,
      body {
        width: 100%;
        height: 100%;
        margin: 0;
        overflow: hidden;
        background: #0d1117;
      }

      .status {
        position: fixed;
        inset: 0;
        display: grid;
        place-items: center;
        color: #d6dde8;
        font: 13px system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        z-index: 1;
      }
    </style>
  </head>
  <body>
    <div class="status" id="openpanels-status">Opening MyOpenPanels...</div>
  </body>
</html>`
}

function injectMcpHostBridge(
  html,
  {
    initialApiBase,
    initialBootstrap,
    initialDisplayMode = "fullscreen",
    initialProjectDir,
    initialStorageDir,
  } = {}
) {
  const bridge = [
    '<script id="openpanelsInitialDisplayMode">',
    `window.__OPENPANELS_INITIAL_DISPLAY_MODE__ = ${JSON.stringify(initialDisplayMode)};`,
    typeof initialApiBase === "string" && initialApiBase
      ? `window.__OPENPANELS_API_BASE__ = ${JSON.stringify(initialApiBase)};`
      : "",
    initialBootstrap
      ? `window.__OPENPANELS_BOOTSTRAP__ = ${escapeInlineJson(JSON.stringify(initialBootstrap))};`
      : "",
    typeof initialProjectDir === "string" && initialProjectDir
      ? `window.__OPENPANELS_PROJECT_DIR__ = ${JSON.stringify(initialProjectDir)};`
      : "",
    typeof initialStorageDir === "string" && initialStorageDir
      ? `window.__OPENPANELS_STORAGE_DIR__ = ${JSON.stringify(initialStorageDir)};`
      : "",
    "</script>",
    '<script id="openpanelsMcpAppsBundle">',
    escapeInlineScript(mcpAppsGlobalScript()),
    "</script>",
    '<script id="openpanelsMcpHostBridge">',
    mcpHostBridgeScript(),
    "</script>",
  ].join("\n")

  if (html.includes("</head>")) {
    return html.replace("</head>", () => `${bridge}\n</head>`)
  }
  return `${bridge}\n${html}`
}

function mcpAppsGlobalScript() {
  if (cachedMcpAppsGlobalScript) return cachedMcpAppsGlobalScript

  const sourcePath = require.resolve(
    "@modelcontextprotocol/ext-apps/app-with-deps"
  )
  const source = readFileSync(sourcePath, "utf8")
  const exportStart = source.lastIndexOf("export{")
  if (exportStart === -1) {
    throw new Error("Could not find ext-apps browser export block.")
  }

  const exportBlock = source.slice(exportStart).match(EXPORT_BLOCK_REGEX)
  if (!exportBlock) {
    throw new Error("Could not parse ext-apps browser export block.")
  }

  const exportMap = parseExportMap(exportBlock[1])
  const requiredExports = [
    "App",
    "applyDocumentTheme",
    "applyHostFonts",
    "applyHostStyleVariables",
  ]
  for (const name of requiredExports) {
    if (!exportMap.has(name))
      throw new Error(`Missing ext-apps browser export: ${name}`)
  }

  cachedMcpAppsGlobalScript = [
    source.slice(0, exportStart),
    ";globalThis.__OPENPANELS_MCP_APPS__={",
    requiredExports
      .map((name) => `${JSON.stringify(name)}:${exportMap.get(name)}`)
      .join(","),
    "};",
  ].join("")
  return cachedMcpAppsGlobalScript
}

function parseExportMap(body) {
  const exportMap = new Map()
  for (const rawEntry of body.split(",")) {
    const entry = rawEntry.trim()
    if (!entry) continue
    const parts = entry.split(EXPORT_ALIAS_REGEX)
    const local = parts[0]?.trim()
    const exported = (parts[1] || parts[0])?.trim()
    if (local && exported) exportMap.set(exported, local)
  }
  return exportMap
}

function escapeInlineScript(source) {
  return source
    .replaceAll("</script", "<\\/script")
    .replaceAll("</SCRIPT", "<\\/SCRIPT")
}

function escapeInlineJson(source) {
  return source
    .replaceAll("</script", "<\\/script")
    .replaceAll("</SCRIPT", "<\\/SCRIPT")
    .replaceAll("<!--", "<\\!--")
}

function mcpHostBridgeScript() {
  return `(() => {
  "use strict";

  const apps = globalThis.__OPENPANELS_MCP_APPS__;
  if (!apps || typeof apps.App !== "function") return;

  let mcpApp = null;

  function publishHostGlobals(globals) {
    window.openai = Object.assign(window.openai || {}, globals);
    window.dispatchEvent(new CustomEvent("openai:set_globals", {
      detail: { globals: window.openai },
    }));
  }

  function applyHostContext(context) {
    if (!context) return;
    try {
      if (context.theme && typeof apps.applyDocumentTheme === "function") {
        apps.applyDocumentTheme(context.theme);
      }
      if (context.styles?.variables && typeof apps.applyHostStyleVariables === "function") {
        apps.applyHostStyleVariables(context.styles.variables);
      }
      if (context.styles?.css?.fonts && typeof apps.applyHostFonts === "function") {
        apps.applyHostFonts(context.styles.css.fonts);
      }
    } catch (_error) {
      // Host styling is a progressive enhancement.
    }

    publishHostGlobals({
      hostContext: context,
      displayMode: context.displayMode,
      availableDisplayModes: context.availableDisplayModes,
      widgetInstanceId: context.widgetInstanceId || context.widgetId,
    });
  }

  function handleToolResult(result) {
    const metadata = result && typeof result === "object" ? result._meta || {} : {};
    const payload = metadata.widgetData || result?.structuredContent || result || {};
    publishHostGlobals({
      rawToolResult: result,
      toolOutput: payload,
      toolResponseMetadata: metadata,
    });
  }

  function withTimeout(promise, ms, label) {
    let timer;
    const timeout = new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(label)), ms);
    });
    return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
  }

  function toBridgeError(error) {
    if (error instanceof Error) return error;
    return new Error(String(error || "MyOpenPanels host bridge is unavailable."));
  }

  async function waitForReady(app) {
    if (app?.ready) {
      await withTimeout(app.ready, 4000, "MyOpenPanels host bridge did not become ready.");
    }
    if (globalThis.__OPENPANELS_MCP_HOST_ERROR__) {
      throw toBridgeError(globalThis.__OPENPANELS_MCP_HOST_ERROR__);
    }
  }

  function installOpenPanelsApi(app) {
    const api = window.openPanelsMcp || {};
    window.openPanelsMcp = api;

    api.callServerTool = async (request, options) => {
      try {
        if (!app || typeof app.callServerTool !== "function") {
          throw new Error("Host tool bridge is unavailable.");
        }
        await waitForReady(app);
        return await withTimeout(
          app.callServerTool(request, options),
          options?.timeoutMs || 30000,
          "MyOpenPanels server tool call timed out.",
        );
      } catch (error) {
        throw toBridgeError(error);
      }
    };

    api.getHostCapabilities = () => {
      try {
        return typeof app?.getHostCapabilities === "function" ? app.getHostCapabilities() : null;
      } catch (_error) {
        return null;
      }
    };

    api.requestDisplayMode = async (modeOrRequest) => {
      if (!app || typeof app.requestDisplayMode !== "function") return {};
      const request = typeof modeOrRequest === "string" ? { mode: modeOrRequest } : (modeOrRequest || { mode: "inline" });
      await waitForReady(app);
      return app.requestDisplayMode(request);
    };
  }

  window.addEventListener("message", (event) => {
    const result = event.data?.params?.result;
    if (event.data?.method === "ui/notifications/tool-result" && result) {
      handleToolResult(result);
    }
  });

  try {
    mcpApp = new apps.App(
      { name: "myopenpanels", version: "0.1.6" },
      { availableDisplayModes: ["inline", "fullscreen"] },
      { autoResize: true },
    );
    globalThis.__OPENPANELS_MCP_APP__ = mcpApp;
    installOpenPanelsApi(mcpApp);

    mcpApp.addEventListener("hostcontextchanged", applyHostContext);
    mcpApp.addEventListener("toolresult", handleToolResult);
    mcpApp.ready = mcpApp.connect()
      .then(() => {
        installOpenPanelsApi(mcpApp);
        publishHostGlobals({
          hostCapabilities: mcpApp.getHostCapabilities && mcpApp.getHostCapabilities(),
          hostInfo: mcpApp.getHostVersion && mcpApp.getHostVersion(),
        });
        applyHostContext(mcpApp.getHostContext && mcpApp.getHostContext());
        if (window.__OPENPANELS_INITIAL_DISPLAY_MODE__ === "fullscreen" && typeof mcpApp.requestDisplayMode === "function") {
          mcpApp.requestDisplayMode({ mode: "fullscreen" }).catch(() => {});
        }
      })
      .catch((error) => {
        globalThis.__OPENPANELS_MCP_HOST_ERROR__ = error;
        publishHostGlobals({ hostBridgeError: String(error?.message || error) });
      });
  } catch (error) {
    globalThis.__OPENPANELS_MCP_HOST_ERROR__ = error;
    publishHostGlobals({ hostBridgeError: String(error?.message || error) });
  }
})();`
}
