import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import type { PublishingAttempt } from "../../types"
import { PublishingAttemptRow } from "./PublishingAttemptRow"

const draftListUrl =
  "https://mp.weixin.qq.com/cgi-bin/appmsg?t=media/appmsg_list&action=list_card&begin=0&count=10&type=10&lang=zh_CN"

function attempt(
  overrides: Partial<PublishingAttempt> = {}
): PublishingAttempt {
  return {
    completedAt: "2026-07-28T08:00:01Z",
    createdAt: "2026-07-28T08:00:00Z",
    id: "attempt:wechat",
    mode: "direct",
    outcome: "published",
    phase: "completed",
    publishedAt: "2026-07-28T08:00:01Z",
    reasonCode: null,
    remoteUrl: draftListUrl,
    requestId: "request:wechat",
    skillHash: "built-in",
    skillId: "release-wechat-official-account",
    skillName: "WeChat Official Account Draft API",
    summary: "Draft saved",
    taskId: null,
    ...overrides,
  }
}

const translate = (value: TemplateStringsArray) => value[0]

describe("PublishingAttemptRow", () => {
  it("shows the draft-list link immediately before the success status", () => {
    const markup = renderToStaticMarkup(
      <PublishingAttemptRow attempt={attempt()} t={translate} />
    )

    expect(markup).toContain(`href="${draftListUrl.replaceAll("&", "&amp;")}"`)
    expect(markup).toContain('target="_blank"')
    expect(markup).toContain(">View<")
    expect(markup.indexOf("op-publishing-attempt__view")).toBeLessThan(
      markup.indexOf("op-publishing-attempt__status")
    )
  })

  it("does not show a link when the submission did not succeed", () => {
    const markup = renderToStaticMarkup(
      <PublishingAttemptRow
        attempt={attempt({
          outcome: "not_published",
          publishedAt: null,
          remoteUrl: draftListUrl,
        })}
        t={translate}
      />
    )

    expect(markup).not.toContain("op-publishing-attempt__view")
  })
})
