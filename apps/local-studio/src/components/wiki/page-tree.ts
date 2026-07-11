import type { WikiPageIndexItem } from "../../types"

export interface WikiPageTreeFolder {
  children: WikiPageTreeNode[]
  kind: "folder"
  name: string
  path: string
}

export interface WikiPageTreePage {
  fileName: string
  kind: "page"
  page: WikiPageIndexItem
}

export type WikiPageTreeNode = WikiPageTreeFolder | WikiPageTreePage

export function buildWikiPageTree(
  pages: WikiPageIndexItem[]
): WikiPageTreeNode[] {
  const root: WikiPageTreeFolder = {
    children: [],
    kind: "folder",
    name: "",
    path: "",
  }
  const folders = new Map<string, WikiPageTreeFolder>()
  folders.set("", root)

  for (const page of pages) {
    const parts = page.path.split("/").filter(Boolean)
    if (parts.length === 0) continue

    let parent = root
    for (const [index, part] of parts.slice(0, -1).entries()) {
      const path = parts.slice(0, index + 1).join("/")
      let folder = folders.get(path)
      if (!folder) {
        folder = { children: [], kind: "folder", name: part, path }
        folders.set(path, folder)
        parent.children.push(folder)
      }
      parent = folder
    }

    parent.children.push({
      fileName: parts.at(-1) ?? page.path,
      kind: "page",
      page,
    })
  }

  sortWikiPageTree(root.children)
  return root.children
}

function sortWikiPageTree(nodes: WikiPageTreeNode[]) {
  nodes.sort((left, right) => {
    if (left.kind !== right.kind) return left.kind === "folder" ? -1 : 1
    const leftName = left.kind === "folder" ? left.name : left.fileName
    const rightName = right.kind === "folder" ? right.name : right.fileName
    return leftName.localeCompare(rightName, undefined, {
      numeric: true,
      sensitivity: "base",
    })
  })

  for (const node of nodes) {
    if (node.kind === "folder") sortWikiPageTree(node.children)
  }
}
