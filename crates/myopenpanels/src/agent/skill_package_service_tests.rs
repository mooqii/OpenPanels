#[cfg(test)]
mod skill_package_service_tests {
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
            Some("skill-package-service-test"),
        )
        .expect("paths");
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        (temp, paths)
    }

    fn write_custom_skill(
        paths: &MyOpenPanelsPaths,
        skill_id: &str,
        module_kinds: &[&str],
    ) {
        let directory = paths.storage_dir.join("skills").join(skill_id);
        fs::create_dir_all(&directory).expect("skill dir");
        fs::write(
            directory.join("SKILL.md"),
            format!(
                "---\nname: {skill_id}\ndescription: Test portable method.\n---\n\nUse direct prose.\n"
            ),
        )
        .expect("SKILL.md");
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "source": "custom",
                "skillId": skill_id,
                "name": "Test portable method",
                "binding": { "moduleKinds": module_kinds },
            }))
            .expect("manifest"),
        )
        .expect("manifest file");
    }

    #[test]
    fn module_queries_follow_manifest_associations() {
        let (_temp, paths) = test_paths();
        write_custom_skill(
            &paths,
            "custom-distiller-only",
            &["writing-distillation"],
        );

        assert!(!list_writing_agent_skills(&paths)
            .expect("writing skills")
            .iter()
            .any(|skill| skill.skill.id == "custom-distiller-only"));
        assert!(list_writing_distillation_agent_skills(&paths)
            .expect("distillation skills")
            .iter()
            .any(|skill| skill.skill.id == "custom-distiller-only"));
    }

    #[test]
    fn custom_skill_edits_are_atomic_and_reject_platform_contracts() {
        let (_temp, paths) = test_paths();
        let skill_id = "custom-editable";
        write_custom_skill(&paths, skill_id, &["writing"]);
        let skill_path = paths.storage_dir.join("skills").join(skill_id).join("SKILL.md");
        let original = fs::read_to_string(&skill_path).expect("original Skill");
        let invalid = original.replace(
            "Use direct prose.",
            "Run myopenpanels agent bootstrap before writing.",
        );

        let error = write_managed_skill_file(&paths, skill_id, "SKILL.md", &invalid)
            .expect_err("platform contract must be rejected");
        assert_eq!(error.code(), Some("skill_file_invalid"));
        assert_eq!(
            fs::read_to_string(&skill_path).expect("unchanged Skill"),
            original
        );

        let edited = original.replace("Use direct prose.", "Use concise prose.");
        write_managed_skill_file(&paths, skill_id, "SKILL.md", &edited).expect("valid edit");
        assert_eq!(
            fs::read_to_string(&skill_path).expect("edited Skill"),
            edited
        );
    }

    #[test]
    fn deletion_clears_every_selected_module_association() {
        let (_temp, paths) = test_paths();
        let initial =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("initial bootstrap");
        let project_id = initial.project.id;
        let panel = |kind| {
            ensure_project_bootstrap(
                &paths,
                BootstrapRequest {
                    requested_panel_kind: Some(kind),
                    requested_panel_id: None,
                    requested_project_id: Some(project_id.clone()),
                },
            )
            .expect("panel bootstrap")
        };
        let wiki = panel(PanelKind::Wiki);
        let writing = panel(PanelKind::Writing);
        let publishing = panel(PanelKind::Publishing);
        let wiki_panel_id = wiki.panel.id;
        let writing_panel_id = writing.panel.id;
        let publishing_panel_id = publishing.panel.id;
        let skill_id = "custom-multi-module";
        write_custom_skill(
            &paths,
            skill_id,
            &["wiki-update", "writing", "writing-distillation", "release"],
        );

        let storage = crate::storage::Storage::open(&paths).expect("storage");
        let mut wiki_state = wiki.state;
        wiki_state["wikiAgentSkillId"] = json!(skill_id);
        storage
            .write_panel_state(&project_id, &wiki_panel_id, &wiki_state)
            .expect("wiki selection");
        let mut writing_state = writing.state;
        writing_state["selectedCreateWritingSkillIds"] = json!([skill_id]);
        writing_state["selectedRevisionWritingSkillId"] = json!(skill_id);
        writing_state["selectedDistillationSkillId"] = json!(skill_id);
        storage
            .write_panel_state(&project_id, &writing_panel_id, &writing_state)
            .expect("writing selections");
        let mut publishing_state = publishing.state;
        publishing_state["selectedSkillIds"]["xiaohongshu"] = json!(skill_id);
        storage
            .write_panel_state(&project_id, &publishing_panel_id, &publishing_state)
            .expect("release selection");

        delete_managed_skill(&paths, skill_id).expect("delete Skill");

        let wiki = storage
            .read_panel_state(&project_id, &wiki_panel_id)
            .expect("wiki state")
            .expect("wiki panel state");
        assert_eq!(
            wiki["wikiAgentSkillId"],
            json!(crate::wiki::DEFAULT_WIKI_AGENT_SKILL_ID)
        );
        let writing = storage
            .read_panel_state(&project_id, &writing_panel_id)
            .expect("writing state")
            .expect("writing panel state");
        assert_eq!(
            writing["selectedCreateWritingSkillIds"],
            json!(["writing-default"])
        );
        assert_eq!(
            writing["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
        assert_eq!(
            writing["selectedDistillationSkillId"],
            json!(crate::writing::DEFAULT_WRITING_DISTILLATION_SKILL_ID)
        );
        let publishing = storage
            .read_panel_state(&project_id, &publishing_panel_id)
            .expect("publishing state")
            .expect("publishing panel state");
        assert_eq!(
            publishing["selectedSkillIds"]["xiaohongshu"],
            json!(crate::release::DEFAULT_XIAOHONGSHU_SKILL_ID)
        );
    }
}
