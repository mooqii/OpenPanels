#[test]
fn legacy_panel_skill_ids_are_read_only_aliases_and_old_packages_are_removed() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");

    let aliases = [
        ("myopenpanels-canvas-panel", "canvas-contract.md"),
        ("myopenpanels-wiki-panel", "wiki-contract.md"),
        ("myopenpanels-writing-panel", "writing-contract.md"),
    ];
    for (alias, _) in aliases {
        let directory = storage_dir.join("skills").join(alias);
        fs::create_dir_all(&directory).expect("legacy package");
        fs::write(directory.join("SKILL.md"), "stale").expect("legacy body");
    }

    let listed = crate::agent::list_agent_skills(&paths).expect("Skill list");
    let listed_ids = listed
        .iter()
        .map(|item| item.skill.id.as_str())
        .collect::<Vec<_>>();
    assert!(listed_ids.contains(&"myopenpanels-panels"));
    for (alias, contract) in aliases {
        assert!(!listed_ids.contains(&alias));
        assert!(!storage_dir.join("skills").join(alias).exists());
        let skill = crate::agent::read_agent_skill(&paths, alias, None).expect("legacy alias");
        assert_eq!(skill.skill.id, "myopenpanels-panels");
        assert!(skill.reference_paths[0].ends_with(contract));
    }
}
