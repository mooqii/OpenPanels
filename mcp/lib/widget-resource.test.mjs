import { describe, expect, it } from "vitest"
import {
  assertCspCompatibleStaticHtml,
  localStudioStaticHtml,
} from "./local-studio-static.mjs"
import {
  openPanelsWidgetHtml,
  registerWidgetResource,
} from "./widget-resource.mjs"

describe("registerWidgetResource", () => {
  it("exposes nested iframe domains in widget CSP metadata", async () => {
    let registered
    const server = {
      registerResource(name, uri, config, readCallback) {
        registered = { name, uri, config, readCallback }
        return { name, uri }
      },
    }

    registerWidgetResource(server, {
      name: "myopenpanels-widget",
      uri: "ui://widget/myopenpanels/index.html",
      title: "MyOpenPanels",
      description: "Open MyOpenPanels.",
      connectDomains: ["http://127.0.0.1:*"],
      resourceDomains: ["data:"],
      frameDomains: ["http://127.0.0.1:*"],
      html: "<!doctype html><html></html>",
    })

    expect(registered.config._meta.ui.csp.frameDomains).toEqual([
      "http://127.0.0.1:*",
    ])
    expect(registered.config._meta["openai/widgetCSP"].frame_domains).toEqual([
      "http://127.0.0.1:*",
    ])

    const resource = await registered.readCallback()
    expect(resource.contents[0]._meta.ui.csp.frameDomains).toEqual([
      "http://127.0.0.1:*",
    ])
    expect(
      resource.contents[0]._meta["openai/widgetCSP"].frame_domains
    ).toEqual(["http://127.0.0.1:*"])
  })

  it("renders a sandbox-compatible inline widget shell", () => {
    const html = openPanelsWidgetHtml({
      appHtml:
        '<!doctype html><html><head></head><body><div id="root"></div><script>window.__OPENPANELS_APP__=true;</script></body></html>',
    })

    expect(html).not.toMatch(/<iframe\b/i)
    expect(html).not.toContain("window.location.replace")
    expect(html).toContain("openpanelsMcpHostBridge")
    expect(html).toContain('id="root"')
  })

  it("inlines the local studio build for MCP app sandbox loading", async () => {
    const html = await localStudioStaticHtml()

    assertCspCompatibleStaticHtml(html)
    const shellMarkup = html
      .replace(/<script\b[\s\S]*?<\/script>/gi, "")
      .replace(/<style\b[\s\S]*?<\/style>/gi, "")
    expect(html).toContain('id="root"')
    expect(shellMarkup).not.toMatch(/<script\b[^>]+\bsrc=/i)
    expect(shellMarkup).not.toMatch(/<link\b[^>]+\bhref=/i)
    expect(shellMarkup).not.toMatch(/<script\b[^>]*\btype="module"/i)
  }, 60_000)
})
