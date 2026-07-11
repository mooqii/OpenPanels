import { describe, expect, it } from "vitest"
import type { WikiPageIndexItem } from "../../types"
import { buildWikiPageTree } from "./page-tree"

describe("buildWikiPageTree", () => {
  it("groups wiki pages by every directory in their path", () => {
    const tree = buildWikiPageTree([
      page("index.md"),
      page("concepts/file-sorting.md"),
      page("topics/runtime/storage.md"),
      page("topics/overview.md"),
    ])

    expect(tree).toMatchObject([
      {
        children: [
          {
            fileName: "file-sorting.md",
            kind: "page",
          },
        ],
        kind: "folder",
        name: "concepts",
        path: "concepts",
      },
      {
        children: [
          {
            children: [
              {
                fileName: "storage.md",
                kind: "page",
              },
            ],
            kind: "folder",
            name: "runtime",
            path: "topics/runtime",
          },
          {
            fileName: "overview.md",
            kind: "page",
          },
        ],
        kind: "folder",
        name: "topics",
        path: "topics",
      },
      {
        fileName: "index.md",
        kind: "page",
      },
    ])
  })

  it("ignores empty page paths", () => {
    expect(buildWikiPageTree([page("")])).toEqual([])
  })
})

function page(path: string): WikiPageIndexItem {
  return {
    path,
    summary: "",
    title: path,
    type: "topic",
    updatedAt: "",
  }
}
