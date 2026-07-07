import { execFile } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

const PLUGIN_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const WIDGET_URI = "ui://widget/myopenpanels/panel.html";
const RESOURCE_MIME_TYPE = "text/html;profile=mcp-app";
const TOOL_RENDER_PANEL = "render_myopenpanels_panel";
const TOOL_START_STUDIO = "start_myopenpanels_studio";
const LOCAL_RESOURCE_DOMAINS = ["http://127.0.0.1:*", "http://localhost:*"];

const manifest = JSON.parse(
  readFileSync(join(PLUGIN_ROOT, ".codex-plugin", "plugin.json"), "utf8")
);

const serverInfo = {
  name: manifest.name,
  version: manifest.version,
};

const instructions =
  "Open MyOpenPanels with render_myopenpanels_panel when the host supports native widgets. It starts or reuses the project-local studio and renders it inside a native Codex widget. If native widgets are unavailable, use start_myopenpanels_studio or openpanels-local studio start and open the returned serverUrl in the in-app Browser side panel.";

const resourceMeta = {
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

const outputTemplateMeta = {
  ui: {
    resourceUri: WIDGET_URI,
    visibility: ["model", "app"],
  },
  "ui/resourceUri": WIDGET_URI,
  "openai/outputTemplate": WIDGET_URI,
  "openai/widgetAccessible": true,
  "openai/toolInvocation/invoking": "Opening MyOpenPanels...",
  "openai/toolInvocation/invoked": "MyOpenPanels ready",
};

let inputBuffer = Buffer.alloc(0);

process.stdin.on("data", (chunk) => {
  inputBuffer = Buffer.concat([inputBuffer, chunk]);
  drainInput().catch((error) => {
    writeError(null, -32_603, error instanceof Error ? error.message : String(error));
  });
});

function drainInput() {
  while (true) {
    const headerEnd = inputBuffer.indexOf("\r\n\r\n");
    if (headerEnd === -1) return Promise.resolve();

    const header = inputBuffer.slice(0, headerEnd).toString("utf8");
    const match = header.match(/content-length:\s*(\d+)/i);
    if (!match) throw new Error("Missing Content-Length header.");

    const contentLength = Number(match[1]);
    const messageStart = headerEnd + 4;
    const messageEnd = messageStart + contentLength;
    if (inputBuffer.length < messageEnd) return Promise.resolve();

    const rawMessage = inputBuffer.slice(messageStart, messageEnd).toString("utf8");
    inputBuffer = inputBuffer.slice(messageEnd);
    void handleMessage(JSON.parse(rawMessage));
  }
}

async function handleMessage(message) {
  if (!Object.hasOwn(message, "id")) return;

  try {
    const result = await routeRequest(message.method, message.params ?? {});
    writeMessage({ jsonrpc: "2.0", id: message.id, result });
  } catch (error) {
    writeError(
      message.id,
      -32_000,
      error instanceof Error ? error.message : String(error)
    );
  }
}

async function routeRequest(method, params) {
  switch (method) {
    case "initialize":
      return {
        protocolVersion: params.protocolVersion || "2025-06-18",
        capabilities: {
          resources: {},
          tools: {},
        },
        serverInfo,
        instructions,
      };
    case "ping":
      return {};
    case "tools/list":
      return { tools: listTools() };
    case "tools/call":
      return callTool(params.name, params.arguments ?? {});
    case "resources/list":
      return { resources: [widgetResource()] };
    case "resources/read":
      if (params.uri !== WIDGET_URI) {
        throw new Error(`Unknown resource: ${params.uri}`);
      }
      return {
        contents: [
          {
            uri: WIDGET_URI,
            mimeType: RESOURCE_MIME_TYPE,
            text: widgetHtml(),
            _meta: resourceMeta,
          },
        ],
      };
    default:
      throw new Error(`Unsupported MCP method: ${method}`);
  }
}

function listTools() {
  return [
    {
      name: TOOL_RENDER_PANEL,
      title: "Render MyOpenPanels Panel",
      description:
        "Start or reuse the MyOpenPanels local studio and open it in a native Codex widget for the active project.",
      inputSchema: {
        type: "object",
        properties: {
          projectDir: { type: "string" },
          title: { type: "string" },
          displayMode: {
            type: "string",
            enum: ["fullscreen", "inline"],
          },
        },
        additionalProperties: false,
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
      _meta: outputTemplateMeta,
    },
    {
      name: TOOL_START_STUDIO,
      title: "Start MyOpenPanels Studio",
      description:
        "Start or reuse the project-local MyOpenPanels studio and return its localhost URL for browser fallback.",
      inputSchema: {
        type: "object",
        properties: {
          projectDir: { type: "string" },
        },
        additionalProperties: false,
      },
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
  ];
}

async function callTool(name, args) {
  if (name === TOOL_START_STUDIO) {
    const studio = await startStudio(args.projectDir);
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

  if (name === TOOL_RENDER_PANEL) {
    const studio = await startStudio(args.projectDir);
    const title = args.title?.trim() || "MyOpenPanels";
    const preferredDisplayMode = args.displayMode || "fullscreen";
    const widgetData = {
      version: 1,
      widget: "myopenpanels-panel-widget",
      title,
      rendering: "native-widget",
      preferredDisplayMode,
      ...studio,
    };

    return {
      content: [
        {
          type: "text",
          text: `Rendered MyOpenPanels native widget at ${studio.serverUrl}.`,
        },
      ],
      structuredContent: widgetData,
      _meta: {
        "openai/outputTemplate": WIDGET_URI,
        widgetData,
      },
    };
  }

  throw new Error(`Unknown tool: ${name}`);
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

function widgetResource() {
  return {
    uri: WIDGET_URI,
    name: "myopenpanels-panel-widget",
    title: "MyOpenPanels",
    description: "Open the MyOpenPanels local studio in a native Codex widget.",
    mimeType: RESOURCE_MIME_TYPE,
    _meta: resourceMeta,
  };
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

function widgetBridgeScript() {
  return `(() => {
  "use strict";

  let lastServerUrl = "";

  function payloadFromToolResult(result) {
    const metadata = result && typeof result === "object" ? result._meta || {} : {};
    return metadata.widgetData || result?.structuredContent || result || {};
  }

  function publishHostGlobals(globals) {
    window.openai = Object.assign(window.openai || {}, globals);
    window.dispatchEvent(new CustomEvent("openai:set_globals", {
      detail: { globals: window.openai },
    }));
  }

  function render(payload) {
    if (!payload?.serverUrl || payload.serverUrl === lastServerUrl) return;
    lastServerUrl = payload.serverUrl;
    const root = document.getElementById("root");
    if (!root) return;
    const frame = document.createElement("iframe");
    frame.id = "myopenpanels-frame";
    frame.title = payload.title || "MyOpenPanels";
    frame.src = payload.serverUrl;
    root.replaceChildren(frame);
  }

  function handleToolResult(result) {
    const payload = payloadFromToolResult(result);
    publishHostGlobals({
      rawToolResult: result,
      toolOutput: payload,
    });
    render(payload);
  }

  window.addEventListener("message", (event) => {
    const result = event.data?.params?.result;
    if (event.data?.method === "ui/notifications/tool-result" && result) {
      handleToolResult(result);
    }
  });

  window.addEventListener("openai:set_globals", () => {
    render(window.openai?.toolOutput || window.openai?.rawToolResult?.structuredContent);
  });

  window.setInterval(() => {
    render(window.openai?.toolOutput || window.openai?.rawToolResult?.structuredContent);
  }, 100);
})();`;
}

function escapeInlineScript(source) {
  return source
    .replaceAll("</script", "<\\/script")
    .replaceAll("</SCRIPT", "<\\/SCRIPT");
}

function writeError(id, code, message) {
  writeMessage({
    jsonrpc: "2.0",
    id,
    error: { code, message },
  });
}

function writeMessage(message) {
  const json = JSON.stringify(message);
  process.stdout.write(`Content-Length: ${Buffer.byteLength(json)}\r\n\r\n${json}`);
}
