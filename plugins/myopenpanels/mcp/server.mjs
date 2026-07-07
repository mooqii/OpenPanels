import { execFile } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const execFileAsync = promisify(execFile);
const require = createRequire(import.meta.url);

const PLUGIN_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const WIDGET_URI = "ui://widget/myopenpanels/panel.html";
const TOOL_RENDER_PANEL = "render_myopenpanels_panel";
const TOOL_START_STUDIO = "start_myopenpanels_studio";
const LOCAL_RESOURCE_DOMAINS = ["http://127.0.0.1:*", "http://localhost:*"];

const manifest = JSON.parse(
  readFileSync(join(PLUGIN_ROOT, ".codex-plugin", "plugin.json"), "utf8")
);

const server = new McpServer(
  {
    name: manifest.name,
    version: manifest.version,
  },
  {
    instructions:
      "Open MyOpenPanels with render_myopenpanels_panel when the host supports native widgets. It starts or reuses the project-local studio and renders it inside a native Codex widget. If native widgets are unavailable, use start_myopenpanels_studio or openpanels-local studio start and open the returned serverUrl in the in-app Browser side panel.",
  }
);

registerWidgetResource(server);
registerPanelTools(server);

const transport = new StdioServerTransport();
await server.connect(transport);

function registerWidgetResource(mcpServer) {
  const metadata = {
    ui: {
      prefersBorder: false,
      csp: {
        connectDomains: LOCAL_RESOURCE_DOMAINS,
        resourceDomains: LOCAL_RESOURCE_DOMAINS,
      },
    },
    "openai/widgetDescription":
      "A native Codex widget that hosts the project-backed MyOpenPanels local studio.",
    "openai/widgetPrefersBorder": false,
    "openai/widgetCSP": {
      connect_domains: LOCAL_RESOURCE_DOMAINS,
      resource_domains: LOCAL_RESOURCE_DOMAINS,
    },
  };

  registerAppResource(
    mcpServer,
    "myopenpanels-panel-widget",
    WIDGET_URI,
    {
      title: "MyOpenPanels",
      description:
        "Open the MyOpenPanels local studio in a native Codex widget.",
      _meta: metadata,
    },
    async () => ({
      contents: [
        {
          uri: WIDGET_URI,
          mimeType: RESOURCE_MIME_TYPE,
          text: widgetHtml(),
          _meta: metadata,
        },
      ],
    })
  );
}

function registerPanelTools(mcpServer) {
  registerAppTool(
    mcpServer,
    TOOL_RENDER_PANEL,
    {
      title: "Render MyOpenPanels Panel",
      description:
        "Start or reuse the MyOpenPanels local studio and open it in a native Codex widget for the active project.",
      inputSchema: {
        projectDir: z.string().trim().optional(),
        title: z.string().trim().optional(),
        displayMode: z.enum(["fullscreen", "inline"]).optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
      _meta: {
        ui: {
          resourceUri: WIDGET_URI,
          visibility: ["model", "app"],
        },
        "ui/resourceUri": WIDGET_URI,
        "openai/outputTemplate": WIDGET_URI,
        "openai/widgetAccessible": true,
        "openai/toolInvocation/invoking": "Opening MyOpenPanels...",
        "openai/toolInvocation/invoked": "MyOpenPanels ready",
      },
    },
    async (input = {}) => {
      const studio = await startStudio(input.projectDir);
      const title = input.title?.trim() || "MyOpenPanels";
      const preferredDisplayMode = input.displayMode || "fullscreen";

      return {
        content: [
          {
            type: "text",
            text: `Rendered MyOpenPanels native widget at ${studio.serverUrl}.`,
          },
        ],
        structuredContent: {
          version: 1,
          widget: "myopenpanels-panel-widget",
          title,
          rendering: "native-widget",
          preferredDisplayMode,
          ...studio,
        },
        _meta: {
          "openai/outputTemplate": WIDGET_URI,
          widgetData: {
            title,
            rendering: "native-widget",
            preferredDisplayMode,
            ...studio,
          },
        },
      };
    }
  );

  mcpServer.registerTool(
    TOOL_START_STUDIO,
    {
      title: "Start MyOpenPanels Studio",
      description:
        "Start or reuse the project-local MyOpenPanels studio and return its localhost URL for browser fallback.",
      inputSchema: {
        projectDir: z.string().trim().optional(),
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async (input = {}) => {
      const studio = await startStudio(input.projectDir);
      return {
        content: [
          {
            type: "text",
            text: `MyOpenPanels studio ready at ${studio.serverUrl}.`,
          },
        ],
        structuredContent: studio,
      };
    }
  );
}

async function startStudio(projectDir) {
  const resolvedProjectDir = resolve(
    projectDir || process.env.OPENPANELS_PROJECT_DIR || process.cwd()
  );
  const command = resolveCliCommand();
  const args = [
    ...command.prefixArgs,
    "studio",
    "start",
    "--project",
    resolvedProjectDir,
    "--format",
    "json",
  ];

  const { stdout } = await execFileAsync(command.command, args, {
    cwd: resolvedProjectDir,
    env: { ...process.env, FORCE_COLOR: "0" },
    maxBuffer: 1024 * 1024,
  });
  const payload = JSON.parse(stdout);
  if (!payload.ok || typeof payload.serverUrl !== "string") {
    throw new Error(`MyOpenPanels studio did not start: ${stdout}`);
  }
  return payload;
}

function resolveCliCommand() {
  if (process.env.OPENPANELS_LOCAL_CLI_ENTRY) {
    return {
      command: process.execPath,
      prefixArgs: [resolve(process.env.OPENPANELS_LOCAL_CLI_ENTRY)],
    };
  }

  const localEntry = findUp(
    PLUGIN_ROOT,
    join("packages", "local-cli", "dist", "openpanels-local.mjs")
  );
  if (localEntry) {
    return { command: process.execPath, prefixArgs: [localEntry] };
  }

  if (process.env.OPENPANELS_LOCAL_CLI) {
    return { command: process.env.OPENPANELS_LOCAL_CLI, prefixArgs: [] };
  }

  return {
    command: "npx",
    prefixArgs: ["-y", "@openpanels/local-cli@latest"],
  };
}

function findUp(startDir, relativePath) {
  let current = resolve(startDir);
  while (true) {
    const candidate = join(current, relativePath);
    if (existsSync(candidate)) return candidate;
    const parent = dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}

function widgetHtml() {
  return `<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>MyOpenPanels</title>
    <style>
      html,
      body,
      #root {
        width: 100%;
        height: 100%;
        min-height: 100vh;
        margin: 0;
        overflow: hidden;
        background: #f8fafc;
        color: #0f172a;
        font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      }

      #root {
        display: grid;
      }

      iframe {
        width: 100%;
        height: 100%;
        min-height: 100vh;
        border: 0;
        background: white;
      }

      .status {
        place-self: center;
        display: grid;
        gap: 8px;
        text-align: center;
        font-size: 14px;
      }

      .status strong {
        font-size: 15px;
      }
    </style>
    <script>${escapeInlineScript(mcpAppsGlobalScript())}</script>
    <script>${escapeInlineScript(widgetBridgeScript())}</script>
  </head>
  <body>
    <div id="root">
      <div class="status">
        <strong>Opening MyOpenPanels</strong>
        <span>Waiting for the local studio...</span>
      </div>
    </div>
  </body>
</html>`;
}

function mcpAppsGlobalScript() {
  const sourcePath = require.resolve(
    "@modelcontextprotocol/ext-apps/app-with-deps"
  );
  const source = readFileSync(sourcePath, "utf8");
  const exportStart = source.lastIndexOf("export{");
  if (exportStart === -1) {
    throw new Error("Could not find ext-apps browser export block.");
  }
  const exportBlock = source
    .slice(exportStart)
    .match(/^export\{([^}]+)\};?\s*$/s);
  if (!exportBlock) {
    throw new Error("Could not parse ext-apps browser export block.");
  }
  const exportMap = parseExportMap(exportBlock[1]);
  const requiredExports = [
    "App",
    "applyDocumentTheme",
    "applyHostFonts",
    "applyHostStyleVariables",
  ];
  for (const name of requiredExports) {
    if (!exportMap.has(name)) {
      throw new Error(`Missing ext-apps browser export: ${name}`);
    }
  }
  return [
    source.slice(0, exportStart),
    ";globalThis.__MYOPENPANELS_MCP_APPS__={",
    requiredExports
      .map((name) => `${JSON.stringify(name)}:${exportMap.get(name)}`)
      .join(","),
    "};",
  ].join("");
}

function parseExportMap(body) {
  const exportMap = new Map();
  for (const rawEntry of body.split(",")) {
    const entry = rawEntry.trim();
    if (!entry) continue;
    const parts = entry.split(/\s+as\s+/);
    const local = parts[0]?.trim();
    const exported = (parts[1] || parts[0])?.trim();
    if (local && exported) exportMap.set(exported, local);
  }
  return exportMap;
}

function widgetBridgeScript() {
  return `(() => {
  "use strict";

  const apps = globalThis.__MYOPENPANELS_MCP_APPS__;
  let mcpApp = null;

  function payloadFromToolResult(result) {
    const metadata = result && typeof result === "object" ? result._meta || {} : {};
    return metadata.widgetData || result?.structuredContent || {};
  }

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
    } catch (_error) {}
    publishHostGlobals({
      hostContext: context,
      displayMode: context.displayMode,
      availableDisplayModes: context.availableDisplayModes,
      widgetInstanceId: context.widgetInstanceId || context.widgetId,
    });
  }

  function render(payload) {
    if (!payload?.serverUrl) return;
    const root = document.getElementById("root");
    if (!root) return;
    let frame = document.getElementById("myopenpanels-frame");
    if (!frame) {
      frame = document.createElement("iframe");
      frame.id = "myopenpanels-frame";
      frame.title = payload.title || "MyOpenPanels";
      root.replaceChildren(frame);
    }
    if (frame.src !== payload.serverUrl) {
      frame.src = payload.serverUrl;
    }
  }

  function handleToolResult(result) {
    const payload = payloadFromToolResult(result);
    publishHostGlobals({
      rawToolResult: result,
      toolOutput: payload,
    });
    render(payload);
    try {
      mcpApp?.sendSizeChanged?.({
        width: Math.ceil(window.innerWidth || 0),
        height: Math.ceil(window.innerHeight || document.documentElement.scrollHeight || 0),
      });
    } catch (_error) {}
  }

  window.addEventListener("message", (event) => {
    const result = event.data?.params?.result;
    if (event.data?.method === "ui/notifications/tool-result" && result) {
      handleToolResult(result);
    }
  });

  try {
    if (apps?.App) {
      mcpApp = new apps.App(
        { name: "myopenpanels", version: "0.1.0" },
        { availableDisplayModes: ["inline", "fullscreen"] },
        { autoResize: true }
      );
      globalThis.__MYOPENPANELS_MCP_APP__ = mcpApp;
      mcpApp.addEventListener("hostcontextchanged", applyHostContext);
      mcpApp.addEventListener("toolresult", handleToolResult);
      mcpApp.connect().then(() => {
        applyHostContext(mcpApp.getHostContext && mcpApp.getHostContext());
        mcpApp.requestDisplayMode?.({ mode: "fullscreen" }).catch(() => {});
      }).catch(() => {});
    }
  } catch (_error) {}

  render(window.openai?.toolOutput || window.openai?.rawToolResult?.structuredContent);
})();`;
}

function escapeInlineScript(source) {
  return source
    .replaceAll("</script", "<\\/script")
    .replaceAll("</SCRIPT", "<\\/SCRIPT");
}
