#[cfg(test)]
mod recommended_skills_tests {
    use super::*;
    use crate::control::ensure_project_bootstrap;
    use crate::paths::resolve_myopenpanels_paths;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temporary = tempfile::tempdir().expect("temp");
        let project_dir = temporary.path().join("workspace");
        let storage_dir = temporary.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("recommended-skills-test"),
        )
        .expect("paths");
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        (temporary, paths)
    }

    fn registration(module_kinds: &[&str]) -> RecommendedSkillRegistration {
        RecommendedSkillRegistration {
            id: "editorial-style".to_owned(),
            name: "editorial-style".to_owned(),
            description: "Keep prose direct.".to_owned(),
            source_url:
                "https://github.com/example/skills/tree/main/catalog/editorial-style".to_owned(),
            module_kinds: module_kinds.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    fn package(root: &Path) {
        fs::create_dir_all(root).expect("package root");
        fs::write(
            root.join("SKILL.md"),
            "---\nname: editorial-style\ndescription: Keep prose direct.\n---\n\nLead with the main point.\n",
        )
        .expect("SKILL.md");
    }

    #[test]
    fn recommended_catalog_validates_and_normalizes_entries() {
        let catalog = load_recommended_skill_catalog(
            r#"{
                "schemaVersion": 1,
                "skills": [{
                    "id": "editorial-style",
                    "name": "Editorial Style",
                    "description": "Keep prose direct.",
                    "sourceUrl": "https://github.com/example/skills/tree/main/editorial",
                    "moduleKinds": ["writing", "publishing-xiaohongshu", "publishing"]
                }]
            }"#,
        )
        .expect("catalog");
        assert_eq!(catalog.skills[0].module_kinds, ["writing", "publishing"]);

        for invalid in [
            r#"{"schemaVersion":2,"skills":[]}"#,
            r#"{"schemaVersion":1,"skills":[{"id":"Bad Id","name":"A","description":"A","sourceUrl":"https://github.com/example/a","moduleKinds":["writing"]}]}"#,
            r#"{"schemaVersion":1,"skills":[{"id":"a","name":"A","description":"A","sourceUrl":"https://github.com/example/a","moduleKinds":["writing"]},{"id":"a","name":"B","description":"B","sourceUrl":"https://github.com/example/b","moduleKinds":["writing"]}]}"#,
            r#"{"schemaVersion":1,"skills":[{"id":"a","name":"Same Name","description":"A","sourceUrl":"https://github.com/example/a","moduleKinds":["writing"]},{"id":"b","name":"same name","description":"B","sourceUrl":"https://github.com/example/b","moduleKinds":["writing"]}]}"#,
            r#"{"schemaVersion":1,"skills":[{"id":"a","name":"A","description":"A","sourceUrl":"https://example.com/a","moduleKinds":["writing"]}]}"#,
            r#"{"schemaVersion":1,"skills":[{"id":"a","name":"A","description":"A","sourceUrl":"https://github.com/example/a","moduleKinds":[]}]}"#,
            r#"{"schemaVersion":1,"skills":[{"id":"a","name":"A","description":"A","sourceUrl":"https://github.com/example/a","moduleKinds":["unknown"]}]}"#,
        ] {
            assert_eq!(
                load_recommended_skill_catalog(invalid)
                    .expect_err("invalid catalog")
                    .code(),
                Some("invalid_recommended_skill_catalog")
            );
        }
    }

    #[test]
    fn empty_embedded_recommended_catalog_lists_without_network_access() {
        let (_temporary, paths) = test_paths();
        let payload = recommended_skills(&paths).expect("recommended skills");
        assert_eq!(payload["schemaVersion"], 1);
        assert_eq!(payload["skills"], json!([]));
    }

    #[test]
    fn fresh_multi_module_install_writes_provenance_and_is_globally_shared() {
        let (temporary, paths) = test_paths();
        let package_root = temporary.path().join("fresh-package");
        package(&package_root);
        let source = parse_remote_skill_source(&registration(&[]).source_url)
            .expect("source")
            .provenance;
        let source_locator = source.source_locator.clone();
        let installed = install_skill_package(
            &paths,
            &package_root,
            "editorial-style".to_owned(),
            "Keep prose direct.".to_owned(),
            &["writing".to_owned(), "publishing".to_owned()],
            false,
            &source_locator,
            Some(source),
        )
        .expect("installed");
        let skill_id = installed["skill"]["id"].as_str().expect("skill id");
        assert_eq!(
            installed["skill"]["moduleKinds"],
            json!(["writing", "publishing"])
        );
        let listing = managed_skill_listing(&paths, skill_id).expect("listing");
        let manifest = read_skill_manifest(&listing).expect("manifest");
        assert_eq!(manifest["schemaVersion"], MANAGED_SKILL_SCHEMA_VERSION);
        assert_eq!(manifest["provenance"]["sourceType"], "github");
        assert!(manifest["provenance"]["installedContentHash"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));

        let other_project = temporary.path().join("other-workspace");
        fs::create_dir_all(&other_project).expect("other project");
        let other_paths = resolve_myopenpanels_paths(
            Some(other_project.to_str().unwrap()),
            Some(paths.storage_dir.to_str().unwrap()),
            Some("recommended-skills-other-project"),
        )
        .expect("other paths");
        ensure_project_bootstrap(&other_paths, BootstrapRequest::new()).expect("other bootstrap");
        let shared = managed_skill_listing(&other_paths, skill_id).expect("shared listing");
        assert_eq!(shared.skill.id, skill_id);
        assert_eq!(managed_skill_module_kinds(&shared), ["writing", "publishing"]);
    }

    #[test]
    fn recommended_state_and_install_share_source_and_merge_all_bindings() {
        let (temporary, paths) = test_paths();
        let package_root = temporary.path().join("package");
        package(&package_root);
        let source = parse_remote_skill_source(&registration(&[]).source_url)
            .expect("source")
            .provenance;
        let source_locator = source.source_locator.clone();
        let installed = install_skill_package(
            &paths,
            &package_root,
            "editorial-style".to_owned(),
            "Keep prose direct.".to_owned(),
            &["writing".to_owned()],
            false,
            &source_locator,
            Some(source),
        )
        .expect("installed");
        let skill_id = installed["skill"]["id"].as_str().unwrap().to_owned();
        let listing = managed_skill_listing(&paths, &skill_id).expect("listing");
        let before = fs::read_to_string(PathBuf::from(&listing.local_dir).join("SKILL.md"))
            .expect("before");
        let state = recommended_skill_listing(
            registration(&["writing", "publishing"]),
            Some(&listing),
        )
        .expect("state");
        assert!(matches!(
            state.install_status,
            RecommendedSkillInstallStatus::BindingsMissing
        ));
        assert_eq!(state.missing_module_kinds, ["publishing"]);

        let associated = install_recommended_skill_registration(
            &paths,
            registration(&["writing", "publishing"]),
        )
        .expect("associated");
        assert_eq!(associated["operation"], "associated");
        assert_eq!(associated["skill"]["id"], skill_id);
        assert_eq!(
            associated["skill"]["moduleKinds"],
            json!(["writing", "publishing"])
        );
        assert_eq!(
            fs::read_to_string(PathBuf::from(&listing.local_dir).join("SKILL.md"))
                .expect("after"),
            before
        );

        let unchanged = install_recommended_skill_registration(
            &paths,
            registration(&["writing", "publishing"]),
        )
        .expect("unchanged");
        assert_eq!(unchanged["operation"], "unchanged");
    }

    #[test]
    fn recommended_install_rejects_unmanaged_same_name_and_name_changes() {
        let (temporary, paths) = test_paths();
        let package_root = temporary.path().join("local-package");
        package(&package_root);
        install_skill_package(
            &paths,
            &package_root,
            "editorial-style".to_owned(),
            "Keep prose direct.".to_owned(),
            &["writing".to_owned()],
            false,
            "local-folder",
            None,
        )
        .expect("local install");
        assert_eq!(
            install_recommended_skill_registration(&paths, registration(&["writing"]))
                .expect_err("source conflict")
                .code(),
            Some("recommended_skill_conflict")
        );
        assert_eq!(
            validate_recommended_skill_name("renamed-style", "editorial-style")
                .expect_err("name mismatch")
                .code(),
            Some("recommended_skill_name_mismatch")
        );
    }
}
