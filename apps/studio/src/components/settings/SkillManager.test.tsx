import { describe, expect, it } from "vitest"
import type {
  DeviceSkillGroup,
  ManagedProjectSkill,
  RecommendedSkill,
} from "../../types"
import {
  groupRecommendedSkills,
  recommendedSkillAction,
  recommendedSkillPresentation,
} from "./RecommendedSkillsPanel"
import {
  canInstallSkill,
  DEFAULT_ADD_SKILL_SOURCE_TAB,
  filterDeviceSkills,
  installedSkillCountLabel,
  managedSkillActionIds,
  moduleLabel,
  scannedSkillAssignments,
  skillUpdatePresentation,
  visibleInstalledSkillCount,
} from "./SkillManager"
import { shouldAutoCheckSkillUpdates } from "./useSkillUpdates"

describe("SkillManagerDialog", () => {
  it("maps read-only and custom Skills to the permitted actions", () => {
    const skill = {
      canCheckUpdates: false,
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
    ).toEqual(["open", "modules", "delete"])
  })

  it("offers updates only when the source has changed", () => {
    const skill = {
      canCheckUpdates: true,
      canDelete: true,
      canEdit: true,
      description: "Description",
      id: "example",
      kind: "custom",
      localDir: "/tmp/example",
      moduleKinds: ["writing"],
      name: "Example",
      provenance: {
        sourceLocator: "https://github.com/example/example",
        sourceType: "github",
      },
    } satisfies ManagedProjectSkill
    const state = {
      checkedAt: "2026-07-21T00:00:00Z",
      localModified: true,
      skillId: skill.id,
      status: "updateAvailable",
    } as const

    expect(managedSkillActionIds(skill, state)).toEqual([
      "open",
      "modules",
      "update",
      "delete",
    ])
    const t = (value: TemplateStringsArray) => value[0] ?? ""
    expect(skillUpdatePresentation(skill, state, false, t)).toEqual({
      label: "Update available · Local changes",
      tone: "warning",
    })
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

  it("excludes hidden MyOpenPanels system Skills from the installed count", () => {
    const skill = {
      canCheckUpdates: false,
      canDelete: false,
      canEdit: false,
      description: "Description",
      id: "shared-skill",
      kind: "preset",
      localDir: "/tmp/shared-skill",
      moduleKinds: ["writing", "publishing"],
      name: "Shared Skill",
    } satisfies ManagedProjectSkill

    expect(
      visibleInstalledSkillCount([
        { kind: "writing", skills: [skill] },
        { kind: "publishing", skills: [skill] },
      ])
    ).toBe(1)
    expect(visibleInstalledSkillCount([])).toBe(0)
  })

  it("requires a module before installing any Skill source", () => {
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
        moduleKind: "writing-distillation",
        sourceType: "zip",
        url: "",
        zipSelected: true,
      })
    ).toBe(true)
  })

  it("initializes every scanned Skill with the launch module", () => {
    expect(
      scannedSkillAssignments(
        [
          { description: "A", name: "alpha", subpath: "skills/alpha" },
          { description: "B", name: "beta", subpath: "skills/beta" },
        ],
        "writing"
      )
    ).toEqual({
      "skills/alpha": "writing",
      "skills/beta": "writing",
    })
  })

  it("presents publishing Skills under the shared content publishing module", () => {
    const t = (value: TemplateStringsArray) => value[0] ?? ""
    expect(moduleLabel("release", t)).toBe("Content publishing")
    expect(moduleLabel("unassociated", t)).toBe("Unassociated Skills")
  })

  it("opens Add Skill on recommendations and checks updates once per opening", () => {
    expect(DEFAULT_ADD_SKILL_SOURCE_TAB).toBe("recommended")
    expect(shouldAutoCheckSkillUpdates("add", false)).toBe(true)
    expect(shouldAutoCheckSkillUpdates("installed", false)).toBe(true)
    expect(shouldAutoCheckSkillUpdates("add", true)).toBe(false)
    expect(shouldAutoCheckSkillUpdates("device", false)).toBe(false)
  })

  it("groups multi-module recommendations while sharing their identity and state", () => {
    const recommended = {
      canCheckUpdates: true,
      description: "Direct editorial prose",
      id: "editorial-style",
      installStatus: "bindingsMissing",
      installedModuleKinds: ["writing"],
      installedSkillId: "custom-editorial",
      missingModuleKinds: ["release"],
      moduleKinds: ["writing", "release"],
      name: "Editorial Style",
      sourceLocator:
        "https://github.com/example/skills/tree/main/editorial-style",
      sourceType: "github",
      sourceUrl: "https://github.com/example/skills/tree/main/editorial-style",
    } satisfies RecommendedSkill
    const groups = groupRecommendedSkills([recommended], "release")

    expect(groups.map((group) => group.kind)).toEqual(["release", "writing"])
    expect(groups[0]?.skills[0]).toBe(recommended)
    expect(groups[1]?.skills[0]).toBe(recommended)
    expect(recommendedSkillAction(recommended)).toBe("associate")
    expect(
      recommendedSkillAction(recommended, {
        checkedAt: "2026-07-21T00:00:00Z",
        localModified: true,
        skillId: "custom-editorial",
        status: "updateAvailable",
      })
    ).toBe("update")
  })

  it("maps recommended installation and update states", () => {
    const recommended = {
      canCheckUpdates: false,
      description: "Direct editorial prose",
      id: "editorial-style",
      installStatus: "notInstalled",
      installedModuleKinds: [],
      missingModuleKinds: [],
      moduleKinds: ["writing"],
      name: "Editorial Style",
      sourceLocator: "https://github.com/example/editorial-style",
      sourceType: "github",
      sourceUrl: "https://github.com/example/editorial-style",
    } satisfies RecommendedSkill
    const t = (value: TemplateStringsArray) => value[0] ?? ""

    expect(
      recommendedSkillPresentation(recommended, undefined, false, false, t)
    ).toEqual({ label: "Not installed", tone: "neutral" })
    expect(
      recommendedSkillPresentation(recommended, undefined, false, true, t)
    ).toEqual({ label: "Installing", tone: "checking" })
    expect(
      recommendedSkillPresentation(
        { ...recommended, installStatus: "conflict" },
        undefined,
        false,
        false,
        t
      )
    ).toEqual({ label: "Skill conflict", tone: "danger" })
  })
})
