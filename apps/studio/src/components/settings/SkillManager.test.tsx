import { describe, expect, it } from "vitest"
import type { DeviceSkillGroup, ManagedProjectSkill } from "../../types"
import {
  canInstallSkill,
  filterDeviceSkills,
  installedSkillCountLabel,
  managedSkillActionIds,
} from "./SkillManager"

describe("SkillManagerDialog", () => {
  it("maps read-only and custom Skills to the permitted actions", () => {
    const skill = {
      canDelete: false,
      canEdit: false,
      description: "Description",
      id: "example",
      kind: "preset",
      localDir: "/tmp/example",
      moduleKinds: ["writing"],
      name: "Example",
    } satisfies ManagedProjectSkill

    expect(managedSkillActionIds(skill)).toEqual(["open"])
    expect(
      managedSkillActionIds({
        ...skill,
        canDelete: true,
        canEdit: true,
        kind: "custom",
      })
    ).toEqual(["open", "delete"])
  })

  it("searches the selected device location and clears back to all Skills", () => {
    const skills = [
      {
        description: "Fallback",
        installed: null,
        key: "house style",
        locations: [
          {
            agents: ["Codex"],
            comparison: "not-installed",
            contentHash: "abc",
            description: "Direct editorial prose",
            path: "/Users/demo/.codex/skills/house-style",
            scope: "global",
            skillPath: "/Users/demo/.codex/skills/house-style/SKILL.md",
          },
        ],
        name: "House Style",
      },
    ] satisfies DeviceSkillGroup[]

    expect(filterDeviceSkills(skills, {}, "editorial")).toHaveLength(1)
    expect(filterDeviceSkills(skills, {}, "CODEX")).toHaveLength(1)
    expect(filterDeviceSkills(skills, {}, "missing")).toHaveLength(0)
    expect(filterDeviceSkills(skills, {}, "")).toEqual(skills)
  })

  it("shows the discovered Skill count in the search placeholder", () => {
    expect(installedSkillCountLabel(23, "zh-CN")).toBe("23个已安装的Skill")
    expect(installedSkillCountLabel(1, "en")).toBe("1 installed Skill")
    expect(installedSkillCountLabel(23, "en")).toBe("23 installed Skills")
  })

  it("requires both an import source and an associated module", () => {
    expect(
      canInstallSkill({
        folderFileCount: 0,
        moduleKind: "",
        sourceType: "url",
        url: "https://github.com/example/skill",
        zipSelected: false,
      })
    ).toBe(false)
    expect(
      canInstallSkill({
        folderFileCount: 0,
        moduleKind: "writing",
        sourceType: "url",
        url: "https://github.com/example/skill",
        zipSelected: false,
      })
    ).toBe(true)
    expect(
      canInstallSkill({
        folderFileCount: 2,
        moduleKind: "wiki-update",
        sourceType: "folder",
        url: "",
        zipSelected: false,
      })
    ).toBe(true)
    expect(
      canInstallSkill({
        folderFileCount: 0,
        moduleKind: "writing-refinement",
        sourceType: "zip",
        url: "",
        zipSelected: true,
      })
    ).toBe(true)
  })
})
