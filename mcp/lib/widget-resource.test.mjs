import { describe, expect, it } from "vitest"
import { registerWidgetResource } from "./widget-resource.mjs"

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
})
