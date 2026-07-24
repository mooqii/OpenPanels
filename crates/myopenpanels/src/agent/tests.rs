#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_json_limits_utf8_depth_arrays_and_objects() {
        let mut object = serde_json::Map::new();
        for index in 0..40 {
            object.insert(format!("field{index:02}"), json!(index));
        }
        object.insert("long".to_owned(), json!("界".repeat(200)));
        object.insert(
            "array".to_owned(),
            Value::Array((0..20).map(|value| json!(value)).collect()),
        );
        object.insert(
            "deep".to_owned(),
            json!({ "one": { "two": { "three": { "four": true } } } }),
        );
        let mut truncated = false;
        let bounded = bounded_json(Value::Object(object), 0, &mut truncated);

        assert!(truncated);
        assert!(serde_json::to_vec(&bounded).expect("json").len() < 1024);
        assert!(bounded.as_object().unwrap().len() <= 32);
        let mut string_truncated = false;
        let string = bounded_json(json!("界".repeat(200)), 0, &mut string_truncated);
        assert!(string_truncated);
        assert!(string.as_str().unwrap().len() <= 256);
        assert!(string
            .as_str()
            .unwrap()
            .is_char_boundary(string.as_str().unwrap().len()));
    }

    #[test]
    fn compact_operation_references_leave_actions_at_the_response_root() {
        let summary = compact_operation_summary(&[json!({
            "id": "operation:1",
            "intent": "canvas.generation.begin",
            "panelKind": "canvas",
            "status": "active",
        })]);

        assert!(summary["items"][0].get("readAction").is_none());
        assert!(summary["items"][0].get("readCommand").is_none());
    }

    #[test]
    fn recommended_domains_skip_panel_kinds_without_agent_commands() {
        assert_eq!(
            recommended_catalog_domains(PanelKind::Typesetting),
            vec![
                "asset",
                "my-document",
                "operation",
                "panel",
                "publication",
                "task"
            ]
        );
        assert_eq!(
            recommended_catalog_domains(PanelKind::Wiki),
            vec![
                "my-document",
                "operation",
                "panel",
                "task",
                "wiki",
                "wiki-source"
            ]
        );
    }

    #[test]
    fn portable_skill_parser_separates_package_metadata_from_platform_binding() {
        let source = "---\nname: concise-writing\ndescription: Write concise prose.\n---\n\nLead with the main point.\n";
        let portable = parse_portable_skill(source, "SKILL.md").expect("portable Skill");
        assert_eq!(portable.metadata.id, "concise-writing");
        assert_eq!(portable.metadata.source, "portable");
        assert!(portable.metadata.applies_to.is_empty());
        assert!(portable.metadata.task_types.is_empty());
        assert!(portable.metadata.requires_commands.is_empty());

        let coupled = "---\nname: concise-writing\ndescription: Write concise prose.\nappliesTo:\n  - writing\n---\n\nLead with the main point.\n";
        assert!(parse_portable_skill(coupled, "SKILL.md").is_err());
    }

    #[test]
    fn custom_skills_validate_manifest_shape() {
        let skill_id = "writing-custom-example";
        let portable = format!(
            "---\nname: {skill_id}\ndescription: Write concise prose.\n---\n\nLead with the main point.\n"
        );
        let portable_manifest = json!({
            "source": "custom",
            "skillId": skill_id,
            "name": "Example Style",
            "binding": {
                "moduleKinds": ["writing"],
            },
        });
        let portable_skill = custom_agent_skill_from_source(
            &portable,
            "portable/SKILL.md",
            &portable_manifest,
        )
        .expect("portable custom Skill");
        assert_eq!(portable_skill.metadata.name, "Example Style");
        assert_eq!(portable_skill.metadata.applies_to, ["writing"]);
        assert_eq!(portable_skill.metadata.task_types, ["write_my_document"]);

        let invalid_binding = json!({
            "source": "custom",
            "skillId": skill_id,
            "name": "Example Style",
            "binding": {
                "appliesTo": ["writing"],
                "taskTypes": ["write_my_document"],
            },
        });
        assert!(custom_agent_skill_from_source(
            &portable,
            "retired-schema/SKILL.md",
            &invalid_binding,
        )
        .is_err());
    }

    #[test]
    fn registered_builtin_packages_are_standard_and_presets_are_portable() {
        let registry: BuiltinSkillRegistry =
            serde_json::from_str(BUILTIN_SKILL_REGISTRY).expect("registry");
        assert_eq!(registry.system_skills.len(), 2);
        for registration in registry.system_skills {
            let directory = SYSTEM_SKILLS
                .get_dir(&registration.package_dir)
                .unwrap_or_else(|| panic!("missing package {}", registration.package_dir));
            let skill_path = directory.path().join("SKILL.md");
            let source = SYSTEM_SKILLS
                .get_file(&skill_path)
                .and_then(|file| std::str::from_utf8(file.contents()).ok())
                .expect("system SKILL.md");
            parse_portable_skill(source, &skill_path.display().to_string())
                .expect("registered standard system Skill");
        }
        for registration in registry.preset_skills {
            for locale in [None, Some("zh-CN")] {
                let package_path = locale
                    .map(|locale| Path::new(locale).join(&registration.package_dir))
                    .unwrap_or_else(|| PathBuf::from(&registration.package_dir));
                let directory = PRESET_SKILLS
                    .get_dir(&package_path)
                    .unwrap_or_else(|| {
                        panic!(
                            "missing package {} for locale {locale:?}",
                            registration.package_dir
                        )
                    });
                assert_portable_directory(directory, &registration.id);
                let skill_path = directory.path().join("SKILL.md");
                let source = PRESET_SKILLS
                    .get_file(&skill_path)
                    .and_then(|file| std::str::from_utf8(file.contents()).ok())
                    .expect("preset SKILL.md");
                parse_portable_skill(source, &skill_path.display().to_string())
                    .expect("registered portable preset Skill");
            }
        }
    }

    #[test]
    fn registered_agent_procedures_and_task_capabilities_are_valid_and_indexed() {
        let catalog = load_agent_procedures().expect("Agent Procedure catalog");
        let entry_skill = include_str!("../../../../skills/myopenpanels/SKILL.md");
        assert_eq!(catalog.procedures.len(), 19);
        for procedure in catalog.procedures {
            assert!(
                entry_skill.contains(&format!("`{}`", procedure.registration.key)),
                "Entry Skill is missing {}",
                procedure.registration.key
            );
        }
    }

    #[test]
    fn agent_procedure_references_are_nonempty_unique_relative_and_present() {
        let registry: BuiltinSkillRegistry =
            serde_json::from_str(BUILTIN_SKILL_REGISTRY).expect("registry");
        let skill = registry
            .system_skills
            .into_iter()
            .find(|skill| skill.id == PANELS_SKILL_ID)
            .expect("Panels Skill");
        let procedure = load_agent_procedures()
            .expect("Agent Procedure catalog")
            .procedures
            .into_iter()
            .find(|procedure| procedure.skill_id == PANELS_SKILL_ID)
            .expect("Procedure")
            .registration;

        for references in [
            Vec::new(),
            vec!["references/canvas-contract.md".to_owned(); 2],
            vec!["../canvas-contract.md".to_owned()],
        ] {
            let mut invalid = procedure.clone();
            invalid.references = references;
            assert_eq!(
                validate_agent_procedure(&skill, &invalid)
                    .expect_err("invalid references")
                    .code(),
                Some("capability_catalog_invalid")
            );
        }

        let mut missing = procedure;
        missing.references = vec!["references/not-found.md".to_owned()];
        assert_eq!(
            validate_agent_procedure(&skill, &missing)
                .expect_err("missing reference")
                .code(),
            Some("capability_reference_not_found")
        );
    }

    fn assert_portable_directory(directory: &Dir<'_>, skill_id: &str) {
        for file in directory.files() {
            let source = std::str::from_utf8(file.contents()).expect("portable text file");
            assert!(
                !portable_skill_mentions_platform(source),
                "portable Skill {skill_id} contains platform text in {}",
                file.path().display()
            );
        }
        for child in directory.dirs() {
            assert_portable_directory(child, skill_id);
        }
    }
}
