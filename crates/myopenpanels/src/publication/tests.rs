#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{
        create_project, ensure_project_bootstrap, read_active_project_id, BootstrapRequest,
    };
    use crate::paths::resolve_myopenpanels_paths;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        (temp, paths)
    }

    fn panel_id(bootstrap: &crate::types::ProjectBootstrap, kind: PanelKind) -> String {
        bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == kind)
            .expect("panel")
            .panel
            .id
            .clone()
    }

    fn add_canvas_asset(
        paths: &MyOpenPanelsPaths,
        project_id: &str,
        panel_id: &str,
        name: &str,
        duplicate_shape: bool,
    ) -> String {
        let storage = Storage::open(paths).expect("storage");
        let written = storage
            .write_asset_from_buffer(
                project_id,
                panel_id,
                name,
                b"independent image bytes",
                false,
            )
            .expect("asset");
        let mut store = serde_json::Map::new();
        store.insert(
            "asset:image".to_owned(),
            json!({
                "id": "asset:image",
                "typeName": "asset",
                "type": "image",
                "props": {
                    "name": name,
                    "mimeType": "image/png",
                    "w": 640,
                    "h": 480
                },
                "meta": {
                    "assetRef": written.asset_ref,
                    "resourceId": written.resource_id,
                }
            }),
        );
        store.insert(
            "shape:image:1".to_owned(),
            json!({
                "id": "shape:image:1",
                "typeName": "shape",
                "type": "image",
                "index": 2,
                "props": { "assetId": "asset:image" }
            }),
        );
        if duplicate_shape {
            store.insert(
                "shape:image:2".to_owned(),
                json!({
                    "id": "shape:image:2",
                    "typeName": "shape",
                    "type": "image",
                    "index": 1,
                    "props": { "assetId": "asset:image" }
                }),
            );
        }
        storage
            .write_panel_state(
                project_id,
                panel_id,
                &json!({
                    "store": store
                }),
            )
            .expect("canvas state");
        written.asset_ref
    }

    #[test]
    fn canvas_asset_queries_deduplicate_without_changing_active_project() {
        let (_temp, paths) = test_paths();
        let source = create_project(&paths, Some("Source")).expect("source");
        let source_canvas = panel_id(&source, PanelKind::Canvas);
        add_canvas_asset(
            &paths,
            &source.project.id,
            &source_canvas,
            "source.png",
            true,
        );
        let current = create_project(&paths, Some("Current")).expect("current");
        let current_canvas = panel_id(&current, PanelKind::Canvas);
        add_canvas_asset(
            &paths,
            &current.project.id,
            &current_canvas,
            "current.png",
            false,
        );

        let current_assets =
            list_canvas_assets(&paths, &current.project.id, "current").expect("current assets");
        assert_eq!(current_assets["assets"].as_array().unwrap().len(), 1);
        assert_eq!(
            current_assets["assets"][0]["projectId"],
            json!(current.project.id)
        );

        let all = list_canvas_assets(&paths, &current.project.id, "all").expect("all assets");
        let assets = all["assets"].as_array().expect("assets");
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0]["projectId"], json!(current.project.id));
        assert_eq!(assets[1]["projectId"], json!(source.project.id));
        assert_eq!(
            read_active_project_id(&paths).expect("active project"),
            Some(current.project.id)
        );
    }

    #[test]
    fn imported_assets_validate_panel_kinds_and_outlive_the_source_project() {
        let (_temp, paths) = test_paths();
        let source = create_project(&paths, Some("Source")).expect("source");
        let source_canvas = panel_id(&source, PanelKind::Canvas);
        let source_ref = add_canvas_asset(
            &paths,
            &source.project.id,
            &source_canvas,
            "source.png",
            false,
        );
        let target = create_project(&paths, Some("Target")).expect("target");
        let target_canvas = panel_id(&target, PanelKind::Canvas);
        let target_typesetting = panel_id(&target, PanelKind::Typesetting);

        let wrong_target =
            import_canvas_asset(&paths, &target.project.id, &target_canvas, &source_ref)
                .expect_err("canvas target must fail");
        assert_eq!(wrong_target.code(), Some("invalid_target"));

        let imported =
            import_canvas_asset(&paths, &target.project.id, &target_typesetting, &source_ref)
                .expect("import");
        let imported_ref = imported["assetRef"].as_str().expect("asset ref");
        assert_ne!(imported_ref, source_ref);
        let wrong_source = import_canvas_asset(
            &paths,
            &target.project.id,
            &target_typesetting,
            imported_ref,
        )
        .expect_err("typesetting source must fail");
        assert_eq!(wrong_source.code(), Some("invalid_target"));

        Storage::open(&paths)
            .expect("storage")
            .delete_project(&source.project.id)
            .expect("delete source");
        let copied = Storage::open(&paths)
            .expect("storage")
            .read_asset(imported_ref)
            .expect("copied asset");
        assert_eq!(copied, b"independent image bytes");

        let refreshed = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_project_id: Some(target.project.id),
                requested_panel_id: None,
                requested_panel_kind: Some(PanelKind::Typesetting),
            },
        )
        .expect("target remains readable");
        assert_eq!(refreshed.active_panel_kind, PanelKind::Typesetting);
    }

    #[test]
    fn cover_request_snapshots_article_and_skill_and_is_idempotent() {
        let (_temp, paths) = test_paths();
        let bootstrap = create_project(&paths, Some("Covers")).expect("project");
        let typesetting_panel = panel_id(&bootstrap, PanelKind::Typesetting);
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.project.id,
                &typesetting_panel,
                &json!({
                    "publications": [{
                        "id": "publication:cover",
                        "title": "A quiet city",
                        "covers": [],
                        "content": {
                            "type": "doc",
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": "Streets after rain" }]
                            }]
                        },
                        "createdAt": "2026-07-21T00:00:00Z",
                        "updatedAt": "2026-07-21T00:00:00Z"
                    }]
                }),
            )
            .expect("typesetting state");

        let skills = cover_skills(&paths).expect("cover skills");
        assert!(skills
            .iter()
            .any(|listing| listing.skill.id == DEFAULT_COVER_SKILL_ID));
        let created = create_cover_request(
            &paths,
            "publication:cover",
            DEFAULT_COVER_SKILL_ID,
            "Use restrained colors",
            "cover-request:1",
        )
        .expect("cover request");
        let task = &created["task"];
        assert_eq!(task["queue"], json!("publication"));
        assert_eq!(task["type"], json!(COVER_TASK_TYPE));
        assert_eq!(task["capability"], json!(publication_task_capability(COVER_TASK_CAPABILITY_KEY, COVER_TASK_TYPE)));
        assert_eq!(task["targetId"], json!("publication:cover"));
        assert_eq!(task["input"]["snapshot"]["title"], json!("A quiet city"));
        assert_eq!(
            task["input"]["snapshot"]["bodyText"],
            json!("Streets after rain")
        );
        assert_eq!(
            task["input"]["coverSkillSnapshot"]["id"],
            json!(DEFAULT_COVER_SKILL_ID)
        );
        assert!(task["input"]["coverSkillSnapshot"]["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"] == "SKILL.md")));

        let workspace = paths.storage_dir.join("cover-execution-bundle-test");
        let prepared = crate::bridge::prepare_execution_bundle(&paths, task, &workspace)
            .expect("cover execution bundle");
        let instructions = &prepared.bundle.instructions;
        let task_id = task["id"].as_str().expect("task id");
        assert_eq!(
            prepared.bundle.handler_key,
            "handler.publication.cover-generation"
        );
        assert!(instructions.contains("references/publication-contract.md"));
        assert!(instructions.contains("references/publication-cover-generate.md"));
        assert!(instructions.contains(&format!("Task id: `{task_id}`")));
        assert!(instructions.contains(&format!("Task type: `{COVER_TASK_TYPE}`")));
        assert!(instructions.contains(&format!(
            "Task capability: `{}`",
            publication_task_capability(COVER_TASK_CAPABILITY_KEY, COVER_TASK_TYPE)
        )));
        assert!(instructions.contains(&format!("Project id: `{}`", bootstrap.project.id)));
        assert!(instructions.contains(&format!("Panel id: `{typesetting_panel}`")));
        assert!(instructions.contains("Request id: `cover-request:1`"));
        assert!(instructions.contains("Publication id: `publication:cover`"));
        assert!(instructions.contains(&format!("Cover Skill id: `{DEFAULT_COVER_SKILL_ID}`")));
        assert!(instructions.contains("inputs/title.txt"));
        assert!(instructions.contains("inputs/body.txt"));
        assert!(instructions.contains("outputs/cover.png"));
        assert!(instructions.contains("Use restrained colors"));
        assert_eq!(
            prepared.bundle.output_contract["artifacts"][0]["role"],
            json!("publication-cover")
        );
        let message = crate::bridge::render_task_handoff_prompt(
            &prepared.bundle,
            "handoff:cover-test",
            "exact-task",
        );
        assert!(message.contains("# Task Handoff Delivery Contract"));
        assert!(message.contains("task handoff heartbeat --handoff-id"));
        assert!(message.contains("task handoff complete --handoff-id"));
        assert!(message.contains("references/publication-cover-generate.md"));

        let repeated = create_cover_request(
            &paths,
            "publication:cover",
            DEFAULT_COVER_SKILL_ID,
            "A different retry payload is ignored",
            "cover-request:1",
        )
        .expect("idempotent retry");
        assert_eq!(repeated["task"]["id"], task["id"]);
        assert_eq!(
            storage
                .list_tasks(&bootstrap.project.id)
                .expect("tasks")
                .len(),
            1
        );
    }

    #[test]
    fn title_request_appends_generated_candidates_without_changing_selection() {
        let (_temp, paths) = test_paths();
        let bootstrap = create_project(&paths, Some("Titles")).expect("project");
        let typesetting_panel = panel_id(&bootstrap, PanelKind::Typesetting);
        let storage = Storage::open(&paths).expect("storage");
        let state = json!({
            "publications": [{
                "id": "publication:title",
                "title": "Original title",
                "titles": [
                    { "id": "title:primary", "value": "Original title" },
                    { "id": "title:second", "value": "Existing alternative" }
                ],
                "selectedTitleId": "title:primary",
                "covers": [],
                "content": {
                    "type": "doc",
                    "content": [{
                        "type": "paragraph",
                        "content": [{ "type": "text", "text": "A practical guide to quiet city walks." }]
                    }]
                },
                "createdAt": "2026-07-22T00:00:00Z",
                "updatedAt": "2026-07-22T00:00:00Z"
            }]
        });
        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &state)
            .expect("typesetting state");

        assert!(title_skills(&paths)
            .expect("title skills")
            .iter()
            .any(|listing| listing.skill.id == DEFAULT_TITLE_SKILL_ID));
        let created = create_title_request(
            &paths,
            "publication:title",
            DEFAULT_TITLE_SKILL_ID,
            "Keep every option concise",
            "title-request:1",
        )
        .expect("title request");
        let task = &created["task"];
        assert_eq!(task["type"], json!(TITLE_TASK_TYPE));
        assert_eq!(task["capability"], json!(publication_task_capability(TITLE_TASK_CAPABILITY_KEY, TITLE_TASK_TYPE)));
        assert_eq!(task["attemptLimit"], json!(3));
        assert_eq!(task["input"]["snapshot"]["title"], json!("Original title"));
        assert_eq!(
            task["input"]["snapshot"]["existingTitles"],
            json!(["Original title", "Existing alternative"])
        );
        assert_eq!(
            task["input"]["titleSkillSnapshot"]["id"],
            json!(DEFAULT_TITLE_SKILL_ID)
        );

        let workspace = paths.storage_dir.join("title-execution-bundle-test");
        let prepared = crate::bridge::prepare_execution_bundle(&paths, task, &workspace)
            .expect("title execution bundle");
        assert_eq!(
            prepared.bundle.handler_key,
            "handler.publication.title-generation"
        );
        assert!(prepared
            .bundle
            .instructions
            .contains("references/publication-title-generate.md"));
        assert!(prepared
            .bundle
            .instructions
            .contains("inputs/existing-titles.json"));
        assert!(prepared
            .bundle
            .instructions
            .contains("one or more distinct"));
        assert_eq!(
            prepared.bundle.output_contract["artifacts"][0]["role"],
            json!("publication-titles")
        );

        let repeated = create_title_request(
            &paths,
            "publication:title",
            DEFAULT_TITLE_SKILL_ID,
            "This retry is ignored",
            "title-request:1",
        )
        .expect("idempotent title retry");
        assert_eq!(repeated["task"]["id"], task["id"]);

        let task_id = task["id"].as_str().expect("task id");
        let generated = (1..=3)
            .map(|index| json!(format!("Candidate title {index}")))
            .collect::<Vec<_>>();
        let result = json!({
            "runtimeFinalization": {
                "artifacts": [{ "titles": generated }]
            }
        });
        let (_, completed_state) = prepare_task_completion(&paths, task_id, Some(result.clone()))
            .expect("prepare title completion")
            .expect("panel state");
        let publication = &completed_state["publications"][0];
        let titles = publication["titles"].as_array().expect("titles");
        assert_eq!(titles.len(), 5);
        assert_eq!(publication["selectedTitleId"], json!("title:primary"));
        assert_eq!(publication["title"], json!("Original title"));
        assert_eq!(titles[2]["value"], json!("Candidate title 1"));
        assert_eq!(titles[2]["source"]["taskId"], json!(task_id));
        assert_eq!(
            titles[2]["source"]["skillId"],
            json!(DEFAULT_TITLE_SKILL_ID)
        );

        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &completed_state)
            .expect("persist titles");
        let (_, repeated_state) = prepare_task_completion(&paths, task_id, Some(result))
            .expect("repeat title completion")
            .expect("panel state");
        assert_eq!(
            repeated_state["publications"][0]["titles"]
                .as_array()
                .expect("titles")
                .len(),
            5
        );
    }

    #[test]
    fn layout_request_snapshots_content_and_excludes_concurrent_work() {
        let (_temp, paths) = test_paths();
        let bootstrap = create_project(&paths, Some("Layout")).expect("project");
        let typesetting_panel = panel_id(&bootstrap, PanelKind::Typesetting);
        let storage = Storage::open(&paths).expect("storage");
        let content = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "Section body" }]
            }]
        });
        let state = json!({
            "publications": [{
                "id": "publication:layout",
                "title": "Layout target",
                "covers": [],
                "content": content,
                "createdAt": "2026-07-22T00:00:00Z",
                "updatedAt": "2026-07-22T00:00:00Z"
            }]
        });
        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &state)
            .expect("typesetting state");

        assert!(layout_skills(&paths)
            .expect("layout skills")
            .iter()
            .any(|listing| listing.skill.id == DEFAULT_LAYOUT_SKILL_ID));
        let created = create_layout_request(
            &paths,
            "publication:layout",
            DEFAULT_LAYOUT_SKILL_ID,
            "Emphasize sections",
            "layout-request:1",
        )
        .expect("layout request");
        let task = &created["task"];
        assert_eq!(task["type"], json!(LAYOUT_TASK_TYPE));
        assert_eq!(task["capability"], json!(publication_task_capability(LAYOUT_TASK_CAPABILITY_KEY, LAYOUT_TASK_TYPE)));
        assert_eq!(task["attemptLimit"], json!(3));
        assert_eq!(task["input"]["snapshot"]["content"], content);
        assert_eq!(
            task["input"]["snapshot"]["contentHash"],
            json!(hash_json(&content).expect("content hash"))
        );
        let workspace = paths.storage_dir.join("layout-execution-bundle-test");
        let prepared = crate::bridge::prepare_execution_bundle(&paths, task, &workspace)
            .expect("layout execution bundle");
        let instructions = &prepared.bundle.instructions;
        let task_id = task["id"].as_str().expect("task id");
        assert_eq!(
            prepared.bundle.handler_key,
            "handler.publication.content-layout"
        );
        assert!(instructions.contains("references/publication-contract.md"));
        assert!(instructions.contains("references/publication-content-format.md"));
        assert!(instructions.contains(&format!("Task id: `{task_id}`")));
        assert!(instructions.contains(&format!("Task type: `{LAYOUT_TASK_TYPE}`")));
        assert!(instructions.contains(&format!(
            "Task capability: `{}`",
            publication_task_capability(LAYOUT_TASK_CAPABILITY_KEY, LAYOUT_TASK_TYPE)
        )));
        assert!(instructions.contains(&format!("Project id: `{}`", bootstrap.project.id)));
        assert!(instructions.contains(&format!("Panel id: `{typesetting_panel}`")));
        assert!(instructions.contains("Request id: `layout-request:1`"));
        assert!(instructions.contains("Publication id: `publication:layout`"));
        assert!(instructions.contains(&format!("Layout Skill id: `{DEFAULT_LAYOUT_SKILL_ID}`")));
        assert!(instructions.contains("Captured content hash: `sha256:"));
        assert!(instructions.contains("inputs/title.txt"));
        assert!(instructions.contains("inputs/content.json"));
        assert!(instructions.contains("outputs/content.json"));
        assert!(instructions.contains("Emphasize sections"));
        assert_eq!(
            prepared.bundle.output_contract["artifacts"][0]["role"],
            json!("publication-content")
        );
        assert_eq!(
            prepared.bundle.output_contract["artifacts"][0]["mediaTypes"],
            json!(["application/json"])
        );
        let message = crate::bridge::render_task_handoff_prompt(
            &prepared.bundle,
            "handoff:layout-test",
            "exact-task",
        );
        assert!(message.contains("# Task Handoff Delivery Contract"));
        assert!(message.contains("task handoff heartbeat --handoff-id"));
        assert!(message.contains("task handoff complete --handoff-id"));
        assert!(message.contains("references/publication-content-format.md"));
        let repeated = create_layout_request(
            &paths,
            "publication:layout",
            DEFAULT_LAYOUT_SKILL_ID,
            "A retry must reuse the original snapshot",
            "layout-request:1",
        )
        .expect("idempotent layout retry");
        assert_eq!(repeated["task"]["id"], task["id"]);

        let cover = create_cover_request(
            &paths,
            "publication:layout",
            DEFAULT_COVER_SKILL_ID,
            "",
            "cover-request:alongside-layout",
        )
        .expect("cover and layout tasks can coexist");
        assert_eq!(cover["task"]["type"], json!(COVER_TASK_TYPE));

        let conflict = create_layout_request(
            &paths,
            "publication:layout",
            DEFAULT_LAYOUT_SKILL_ID,
            "",
            "layout-request:2",
        )
        .expect_err("active layout must be exclusive");
        assert_eq!(conflict.code(), Some("publication_layout_in_progress"));

        let mut changed = state.clone();
        changed["publications"][0]["content"] = json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Changed" }] }]
        });
        let locked =
            validate_content_write(&paths, &bootstrap.project.id, &typesetting_panel, &changed)
                .expect_err("active layout must lock content");
        assert_eq!(locked.code(), Some("publication_content_locked"));
    }

    #[test]
    fn layout_completion_replaces_only_content_and_detects_stale_sources() {
        let (_temp, paths) = test_paths();
        let bootstrap = create_project(&paths, Some("Layout completion")).expect("project");
        let typesetting_panel = panel_id(&bootstrap, PanelKind::Typesetting);
        let storage = Storage::open(&paths).expect("storage");
        let content = json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": "Section body" }]
            }]
        });
        let state = json!({
            "publications": [{
                "id": "publication:layout-completion",
                "title": "Original title",
                "covers": [{
                    "assetRef": "asset:cover",
                    "fileName": "cover.png",
                    "mimeType": "image/png",
                    "src": "/cover.png",
                    "source": {
                        "kind": "generated",
                        "taskId": "task:cover",
                        "skillId": DEFAULT_COVER_SKILL_ID
                    }
                }],
                "content": content,
                "createdAt": "2026-07-22T00:00:00Z",
                "updatedAt": "2026-07-22T00:00:00Z"
            }]
        });
        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &state)
            .expect("typesetting state");
        let created = create_layout_request(
            &paths,
            "publication:layout-completion",
            DEFAULT_LAYOUT_SKILL_ID,
            "",
            "layout-request:completion",
        )
        .expect("layout request");
        let task_id = created["task"]["id"].as_str().expect("task id");
        let formatted = json!({
            "type": "doc",
            "content": [{
                "type": "heading",
                "attrs": { "level": 2 },
                "content": [{ "type": "text", "text": "Section body" }]
            }]
        });
        let result = json!({
            "runtimeFinalization": {
                "artifacts": [{ "content": formatted }]
            }
        });

        let mut title_changed = state.clone();
        title_changed["publications"][0]["title"] = json!("Edited while layout ran");
        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &title_changed)
            .expect("title edit");
        let (_, completed) = prepare_task_completion(&paths, task_id, Some(result.clone()))
            .expect("prepare completion")
            .expect("panel state");
        assert_eq!(
            completed["publications"][0]["title"],
            json!("Edited while layout ran")
        );
        assert_eq!(
            completed["publications"][0]["covers"],
            state["publications"][0]["covers"]
        );
        assert_eq!(
            completed["publications"][0]["content"],
            result["runtimeFinalization"]["artifacts"][0]["content"]
        );

        let mut stale = title_changed;
        stale["publications"][0]["content"] = json!({
            "type": "doc",
            "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "External edit" }] }]
        });
        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &stale)
            .expect("external content edit");
        let error = prepare_task_completion(&paths, task_id, Some(result))
            .expect_err("stale source must conflict");
        assert_eq!(error.code(), Some("content_conflict"));
    }

    #[test]
    fn cover_completion_appends_once_and_rejects_a_deleted_publication() {
        let (_temp, paths) = test_paths();
        let bootstrap = create_project(&paths, Some("Covers")).expect("project");
        let typesetting_panel = panel_id(&bootstrap, PanelKind::Typesetting);
        let storage = Storage::open(&paths).expect("storage");
        let state = json!({
            "publications": [{
                "id": "publication:cover",
                "title": "Cover target",
                "covers": [],
                "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
                "createdAt": "2026-07-21T00:00:00Z",
                "updatedAt": "2026-07-21T00:00:00Z"
            }]
        });
        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &state)
            .expect("typesetting state");
        let created = create_cover_request(
            &paths,
            "publication:cover",
            DEFAULT_COVER_SKILL_ID,
            "",
            "cover-request:complete",
        )
        .expect("cover request");
        let task_id = created["task"]["id"].as_str().expect("task id");
        let result = json!({
            "runtimeFinalization": {
                "artifacts": [{
                    "assetRef": format!("projects/{}/content/asset/asset:cover/1/cover.png", bootstrap.project.id),
                    "resourceId": "asset:cover",
                    "fileName": format!("cover-tasks/{task_id}/cover.png"),
                    "mimeType": "image/png",
                    "width": 1200,
                    "height": 900
                }]
            }
        });
        let (_, completed_state) = prepare_task_completion(&paths, task_id, Some(result.clone()))
            .expect("prepare completion")
            .expect("panel state");
        let covers = completed_state["publications"][0]["covers"]
            .as_array()
            .expect("covers");
        assert_eq!(covers.len(), 1);
        assert_eq!(covers[0]["source"]["taskId"], json!(task_id));
        assert_eq!(
            covers[0]["source"]["skillId"],
            json!(DEFAULT_COVER_SKILL_ID)
        );

        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &completed_state)
            .expect("persist prepared state");
        let (_, repeated_state) = prepare_task_completion(&paths, task_id, Some(result.clone()))
            .expect("repeat completion")
            .expect("panel state");
        assert_eq!(
            repeated_state["publications"][0]["covers"]
                .as_array()
                .expect("covers")
                .len(),
            1
        );

        storage
            .write_panel_state(
                &bootstrap.project.id,
                &typesetting_panel,
                &json!({ "publications": [] }),
            )
            .expect("delete publication");
        let error = prepare_task_completion(&paths, task_id, Some(result))
            .expect_err("deleted publication must reject completion");
        assert_eq!(error.code(), Some("publication_not_found"));
    }
}
