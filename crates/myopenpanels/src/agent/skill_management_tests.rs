#[cfg(test)]
mod skill_management_tests {
    use super::*;
    use crate::control::ensure_project_bootstrap;
    use crate::paths::resolve_myopenpanels_paths;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("workspace");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("skill-management-test"),
        )
        .expect("paths");
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        (temp, paths)
    }

    fn write_custom_skill(paths: &MyOpenPanelsPaths, skill_id: &str, name: &str) {
        let directory = paths.storage_dir.join("skills").join(skill_id);
        fs::create_dir_all(&directory).expect("skill dir");
        fs::write(
            directory.join("SKILL.md"),
            format!(
                "---\nname: {skill_id}\ndescription: Write direct prose.\n---\n\nLead with the main point.\n"
            ),
        )
        .expect("SKILL.md");
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 2,
                "source": "custom",
                "skillId": skill_id,
                "name": name,
                "binding": {
                    "appliesTo": ["writing"],
                    "taskTypes": ["generate_document"],
                },
            }))
            .expect("manifest"),
        )
        .expect("manifest file");
    }

    #[test]
    fn managed_skills_are_global_and_enforce_permissions() {
        let (_temp, paths) = test_paths();
        write_custom_skill(&paths, "writing-custom-shared", "Shared Style");
        let first_project = crate::control::create_project(&paths, Some("A"))
            .expect("project A")
            .project;
        let second_project = crate::control::create_project(&paths, Some("B"))
            .expect("project B")
            .project;

        for project_id in [first_project.id, second_project.id] {
            let skills = list_agent_skills_for_project(&paths, &project_id).expect("skills");
            assert!(skills
                .iter()
                .any(|skill| skill.skill.id == "writing-custom-shared"));
        }

        let payload = managed_skills(&paths).expect("managed skills");
        let modules = payload["modules"].as_array().unwrap();
        assert!(modules.iter().any(|module| {
            module["kind"] == "wiki-update"
                && module["skills"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|skill| skill["id"] == "karpathy-llm-wiki")
        }));
        assert!(modules.iter().any(|module| {
            module["kind"] == "writing-refinement"
                && module["skills"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|skill| skill["id"] == "writing-skill-refiner")
        }));
        assert!(modules.iter().any(|module| {
            module["kind"] == "writing"
                && module["skills"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|skill| skill["id"] == "writing-default")
        }));
        let all = payload["modules"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|module| module["skills"].as_array().unwrap())
            .collect::<Vec<_>>();
        let preset = all
            .iter()
            .find(|skill| skill["id"] == "writing-default")
            .expect("preset");
        assert_eq!(preset["kind"], "preset");
        assert_eq!(preset["canEdit"], false);
        let custom = all
            .iter()
            .find(|skill| skill["id"] == "writing-custom-shared")
            .expect("custom");
        assert_eq!(custom["canEdit"], true);
        assert_eq!(custom["canDelete"], true);

        let error = write_managed_skill_file(&paths, "writing-default", "SKILL.md", "not allowed")
            .expect_err("preset is read only");
        assert_eq!(error.code(), Some("skill_read_only"));
    }

    #[test]
    fn legacy_custom_skill_moves_into_the_global_skills_directory() {
        let (_temp, paths) = test_paths();
        let legacy = paths.storage_dir.join("writing-skills/legacy-global");
        fs::create_dir_all(&legacy).expect("legacy dir");
        fs::write(
            legacy.join("SKILL.md"),
            "---\nname: legacy-global\ndescription: Legacy global Skill.\n---\n\nWrite clearly.\n",
        )
        .expect("legacy skill");
        fs::write(
            legacy.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 2,
                "source": "custom",
                "skillId": "legacy-global",
                "name": "Legacy Global",
                "binding": {
                    "appliesTo": ["writing"],
                    "taskTypes": ["generate_document"],
                },
            }))
            .unwrap(),
        )
        .expect("legacy manifest");

        migrate_legacy_custom_agent_skills(&paths).expect("migration");
        assert!(paths
            .storage_dir
            .join("skills/legacy-global/SKILL.md")
            .is_file());
        assert!(!legacy.exists());
    }

    #[test]
    fn device_discovery_groups_names_and_deduplicates_symlinks() {
        let temp = tempfile::tempdir().expect("temp");
        let root_a = temp.path().join("agent-a");
        let root_b = temp.path().join("agent-b");
        let skill_a = root_a.join("category/shared");
        let skill_b = root_b.join("shared-copy");
        fs::create_dir_all(&skill_a).expect("skill a");
        fs::create_dir_all(&skill_b).expect("skill b");
        for directory in [&skill_a, &skill_b] {
            fs::write(
                directory.join("SKILL.md"),
                "---\nname: Shared Skill\ndescription: Shared description.\n---\n\nBody.\n",
            )
            .expect("skill file");
        }
        let root_alias = temp.path().join("agent-alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&skill_a, &root_alias).expect("alias");

        let roots = vec![
            DeviceSkillRoot {
                path: root_a,
                scope: "global",
                agent: "Agent A",
            },
            DeviceSkillRoot {
                path: root_b,
                scope: "global",
                agent: "Agent B",
            },
            DeviceSkillRoot {
                path: root_alias,
                scope: "global",
                agent: "Agent Alias",
            },
        ];
        let mut discovered = BTreeMap::new();
        for root in roots {
            let mut visited = BTreeSet::new();
            scan_device_skill_root(&root, &root.path, 0, &mut visited, &mut discovered)
                .expect("scan");
        }
        assert_eq!(discovered.len(), 2);
        let canonical_a = fs::canonicalize(&skill_a).expect("canonical a");
        let shared = discovered.get(&canonical_a).expect("shared instance");
        assert!(shared.agents.contains("Agent A"));
        #[cfg(unix)]
        assert!(shared.agents.contains("Agent Alias"));
    }

    #[test]
    fn device_discovery_scans_workbuddy_skills_and_installed_connectors() {
        let temp = tempfile::tempdir().expect("temp");
        let workbuddy_home = temp.path().join(".workbuddy");
        let skill = workbuddy_home.join("skills/editorial-style");
        let connector_skill = workbuddy_home.join("connectors/skills/company-search");
        let marketplace_skill = workbuddy_home
            .join("connectors-marketplace/connectors/catalog-only/skills/catalog-search");
        for (directory, name) in [
            (&skill, "Editorial Style"),
            (&connector_skill, "Company Search"),
            (&marketplace_skill, "Catalog Search"),
        ] {
            fs::create_dir_all(directory).expect("WorkBuddy Skill directory");
            fs::write(
                directory.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: WorkBuddy Skill.\n---\n\nBody.\n"),
            )
            .expect("WorkBuddy SKILL.md");
        }

        let roots = workbuddy_skill_roots(temp.path());
        let mut discovered = BTreeMap::new();
        for root in roots {
            let mut visited = BTreeSet::new();
            scan_device_skill_root(&root, &root.path, 0, &mut visited, &mut discovered)
                .expect("scan WorkBuddy Skills");
        }

        assert_eq!(discovered.len(), 2);
        assert!(discovered
            .values()
            .all(|instance| instance.agents.contains("WorkBuddy")));
        assert!(discovered
            .values()
            .all(|instance| instance.name != "Catalog Search"));
    }

    #[test]
    fn device_skill_install_adds_and_removes_global_module_associations() {
        let (_temp, paths) = test_paths();
        let device_dir = paths.project_dir.join(".codex/skills/shared-style");
        fs::create_dir_all(&device_dir).expect("device skill dir");
        fs::write(
            device_dir.join("SKILL.md"),
            "---\nname: Shared Style\ndescription: Keep the prose direct.\nlicense: MIT\n---\n\nLead with the main point.\n",
        )
        .expect("device skill");

        install_device_skill(&paths, device_dir.to_str().unwrap(), "writing").expect("install");
        let listing = find_installed_skill_by_identity(&paths, "shared style")
            .expect("identity lookup")
            .expect("installed listing");
        let skill_id = listing.skill.id.clone();
        assert_eq!(managed_skill_module_kinds(&listing), ["writing"]);

        install_device_skill(&paths, device_dir.to_str().unwrap(), "writing-refinement")
            .expect("associate refinement");
        let listing = managed_skill_listing(&paths, &skill_id).expect("listing");
        assert_eq!(
            managed_skill_module_kinds(&listing),
            ["writing", "writing-refinement"]
        );

        fs::write(device_dir.join("notes.txt"), "device revision").expect("device revision");
        let discovered = discover_device_skills(&paths).expect("discover mismatch");
        let mut reached_uninstalled = false;
        for group in discovered["skills"].as_array().unwrap() {
            if group["installed"].is_null() {
                reached_uninstalled = true;
            } else {
                assert!(!reached_uninstalled, "installed Skills must sort first");
            }
        }
        let group = discovered["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["key"] == "shared style")
            .expect("shared group");
        assert_eq!(group["locations"][0]["comparison"], "different");
        ignore_skill_mismatch(
            &paths,
            &skill_id,
            device_dir.to_str().unwrap(),
            group["installed"]["contentHash"].as_str().unwrap(),
            group["locations"][0]["contentHash"].as_str().unwrap(),
        )
        .expect("ignore mismatch");
        let discovered = discover_device_skills(&paths).expect("discover ignored mismatch");
        let group = discovered["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["key"] == "shared style")
            .expect("shared group");
        assert_eq!(group["locations"][0]["comparison"], "ignored");
        fs::write(
            Path::new(&listing.local_dir).join("local.txt"),
            "local revision",
        )
        .expect("local revision");
        let discovered = discover_device_skills(&paths).expect("discover changed local");
        let group = discovered["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["key"] == "shared style")
            .expect("shared group");
        assert_eq!(group["locations"][0]["comparison"], "different");

        remove_skill_module(&paths, &skill_id, "writing").expect("remove writing");
        assert!(Path::new(&listing.local_dir).is_dir());
        remove_skill_module(&paths, &skill_id, "writing-refinement")
            .expect("remove final association");
        assert!(!Path::new(&listing.local_dir).exists());
    }

    #[test]
    fn local_skill_import_validates_conflicts_and_replaces_custom_skills() {
        use base64::Engine;
        let (_temp, paths) = test_paths();
        let source = |body: &str| SkillImportFile {
            path: "house-style/SKILL.md".to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(format!(
                "---\nname: house-style\ndescription: A concise house style.\n---\n\n{body}\n"
            )),
        };

        let installed =
            import_skill_from_files(&paths, &[source("Use short sentences.")], "writing", false)
                .expect("initial import");
        assert_eq!(installed["status"], "installed");
        assert_eq!(installed["replaced"], false);
        let skill_id = installed["skill"]["id"].as_str().unwrap().to_owned();

        let conflict = import_skill_from_files(
            &paths,
            &[source("Lead with the conclusion.")],
            "writing-refinement",
            false,
        )
        .expect("conflict response");
        assert_eq!(conflict["status"], "conflict");
        let before = fs::read_to_string(
            paths
                .storage_dir
                .join("skills")
                .join(&skill_id)
                .join("SKILL.md"),
        )
        .expect("installed source");
        assert!(before.contains("Use short sentences."));

        let replaced = import_skill_from_files(
            &paths,
            &[source("Lead with the conclusion.")],
            "writing-refinement",
            true,
        )
        .expect("replacement import");
        assert_eq!(replaced["status"], "installed");
        assert_eq!(replaced["replaced"], true);
        assert_eq!(
            replaced["skill"]["moduleKinds"],
            json!(["writing", "writing-refinement"])
        );
        let after = fs::read_to_string(
            paths
                .storage_dir
                .join("skills")
                .join(&skill_id)
                .join("SKILL.md"),
        )
        .expect("replaced source");
        assert!(after.contains("Lead with the conclusion."));
    }

    #[test]
    fn skill_import_rejects_invalid_packages_and_builtin_names() {
        use base64::Engine;
        let (_temp, paths) = test_paths();
        let file = |path: &str, source: &str| SkillImportFile {
            path: path.to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(source),
        };

        let invalid = import_skill_from_files(
            &paths,
            &[file("notes.txt", "not a Skill")],
            "writing",
            false,
        )
        .expect_err("missing SKILL.md");
        assert_eq!(invalid.code(), Some("invalid_skill_package"));

        let invalid_name = import_skill_from_files(
            &paths,
            &[file(
                "display-name/SKILL.md",
                "---\nname: Display Name\ndescription: Invalid canonical name.\n---\n\nBody.\n",
            )],
            "writing",
            false,
        )
        .expect_err("non-standard name");
        assert_eq!(invalid_name.code(), Some("invalid_skill_package"));

        let reserved = import_skill_from_files(
            &paths,
            &[file(
                "writing-default/SKILL.md",
                "---\nname: writing-default\ndescription: Collision.\n---\n\nBody.\n",
            )],
            "writing",
            false,
        )
        .expect_err("builtin collision");
        assert_eq!(reserved.code(), Some("skill_reserved_name"));
    }

    #[test]
    fn remote_skill_urls_are_restricted_and_resolve_supported_sources() {
        let parsed = parse_github_skill_source(
            "https://github.com/example/skills/tree/main/catalog/editorial",
        )
        .expect("GitHub tree URL");
        assert_eq!(parsed.owner, "example");
        assert_eq!(parsed.repo, "skills");
        assert_eq!(parsed.revision, "main");
        assert_eq!(parsed.subpath.as_deref(), Some("catalog/editorial"));

        let github = parse_remote_skill_source(
            "https://github.com/example/skills/tree/main/catalog/editorial",
        )
        .expect("GitHub source");
        assert_eq!(
            github.archive_url,
            "https://codeload.github.com/example/skills/zip/main"
        );
        assert_eq!(github.subpath.as_deref(), Some("catalog/editorial"));

        let clawhub = parse_remote_skill_source(
            "https://clawhub.ai/openclaw/skills/memory-tools?version=1.0.0",
        )
        .expect("ClawHub source");
        assert_eq!(
            clawhub.archive_url,
            "https://clawhub.ai/api/v1/download?slug=memory-tools&ownerHandle=openclaw"
        );

        let skillhub =
            parse_remote_skill_source("https://skillhub.cn/skills/github-code-review/#readme")
                .expect("SkillHub source");
        assert_eq!(
            skillhub.archive_url,
            "https://api.skillhub.cn/api/v1/download?slug=github-code-review"
        );

        let plugin =
            parse_remote_skill_source("https://clawhub.ai/openclaw/plugins/memory-lancedb")
                .expect_err("ClawHub Plugin URL");
        assert_eq!(plugin.code(), Some("unsupported_skill_source"));

        let hermes = parse_remote_skill_source("https://hermes-ai.net/skills/")
            .expect_err("Hermes guide URL");
        assert_eq!(hermes.code(), Some("unsupported_skill_source"));

        let error = parse_remote_skill_source("https://example.com/skill.zip")
            .expect_err("unsupported host");
        assert_eq!(error.code(), Some("unsupported_skill_source"));
    }
}
