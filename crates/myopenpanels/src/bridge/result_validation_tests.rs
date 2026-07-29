#[cfg(test)]
mod publication_cover_png_tests {
    use super::*;

    fn append_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(chunk_type);
        png.extend_from_slice(data);
        let start = png.len() - data.len() - chunk_type.len();
        let crc = png_crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }

    fn structurally_valid_png() -> Vec<u8> {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        append_chunk(
            &mut png,
            b"IHDR",
            &[0, 0, 0, 4, 0, 0, 0, 3, 8, 6, 0, 0, 0],
        );
        append_chunk(&mut png, b"IDAT", &[0x78, 0x9c, 0x03, 0x00]);
        append_chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn png_validation_requires_complete_crc_checked_structure() {
        let png = structurally_valid_png();
        assert_eq!(png_dimensions(&png), Some((4, 3)));
        assert_eq!(png_dimensions(&png[..24]), None);

        let mut forged = png;
        forged[20] ^= 1;
        assert_eq!(png_dimensions(&forged), None);
    }

    #[test]
    fn cover_output_plan_accepts_multiple_pngs_and_rejects_unsafe_or_invalid_outputs() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("cover-output-test"),
        )
        .expect("paths");
        let task = json!({
            "id": "task:cover",
            "projectId": "project:cover",
            "panelId": "panel:typesetting"
        });
        let write_result = |relative_paths: Vec<&str>| {
            write_test_result(
                &workspace,
                &json!({
                    "outcome": "generated",
                    "summary": "cover",
                    "artifacts": relative_paths
                        .into_iter()
                        .map(|relative_path| json!({
                            "role": "publication-cover",
                            "relativePath": relative_path
                        }))
                        .collect::<Vec<_>>()
                }),
            )
            .expect("execution result");
        };

        fs::write(workspace.join("outputs/cover.png"), structurally_valid_png())
            .expect("valid png");
        fs::write(workspace.join("outputs/cover-2.png"), structurally_valid_png())
            .expect("second valid png");
        write_result(vec!["outputs/cover.png", "outputs/cover-2.png"]);
        let plan = build_publication_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect("valid output");
        assert_eq!(plan.actions.len(), 2);

        write_result(vec!["../cover.png"]);
        assert!(build_publication_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());

        write_result(vec!["outputs/cover.png"]);
        fs::write(workspace.join("outputs/cover.png"), []).expect("empty png");
        assert!(build_publication_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());
        fs::write(
            workspace.join("outputs/cover.png"),
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        )
        .expect("forged png");
        assert!(build_publication_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());

        let oversized = fs::File::create(workspace.join("outputs/cover.png"))
            .expect("oversized output");
        oversized
            .set_len(crate::content::MAX_TEXT_FILE_BYTES as u64 + 1)
            .expect("sparse file");
        let error = build_publication_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect_err("oversized output must fail");
        assert_eq!(error.code(), Some("content_too_large"));
    }
}

#[cfg(test)]
mod publication_title_output_tests {
    use super::*;

    #[test]
    fn title_output_requires_one_or_more_new_distinct_strings() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("title-output-test"),
        )
        .expect("paths");
        let task = json!({
            "id": "task:title",
            "projectId": "project:title",
            "panelId": "panel:typesetting",
            "input": {
                "snapshot": { "existingTitles": ["Existing title"] }
            }
        });
        write_test_result(
            &workspace,
            &json!({
                "outcome": "generated",
                "summary": "titles",
                "artifacts": [{
                    "role": "publication-titles",
                    "relativePath": "outputs/titles.json"
                }]
            }),
        )
        .expect("execution result");
        let write_titles = |titles: Vec<Value>| {
            fs::write(
                workspace.join("outputs/titles.json"),
                serde_json::to_vec(&json!({ "titles": titles })).expect("serialize titles"),
            )
            .expect("title artifact");
        };

        write_titles(vec![json!("Candidate 1")]);
        let plan = build_publication_title_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect("valid titles");
        assert!(matches!(
            &plan.actions[0],
            TaskOutputAction::PreparePublicationTitles {
                project_id,
                panel_id,
                titles,
                ..
            } if project_id == "project:title"
                && panel_id == "panel:typesetting"
                && titles.len() == 1
        ));

        write_titles(Vec::new());
        assert!(build_publication_title_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());

        write_titles(vec![json!("Candidate 1"), json!("candidate 1")]);
        assert!(build_publication_title_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());

        write_titles(vec![json!(" existing TITLE ")]);
        assert!(build_publication_title_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());
    }

    #[test]
    fn title_output_is_persisted_in_the_task_project_and_panel() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("title-output-target-test"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest {
                requested_panel_kind: Some(crate::types::PanelKind::Typesetting),
                ..crate::control::BootstrapRequest::new()
            },
        )
        .expect("bootstrap");
        let task = json!({
            "id": "task:title-target",
            "projectId": bootstrap.project.id,
            "panelId": bootstrap.panel.id,
            "input": {
                "snapshot": { "existingTitles": ["Existing title"] }
            }
        });
        write_test_result(
            &workspace,
            &json!({
                "outcome": "generated",
                "summary": "titles",
                "artifacts": [{
                    "role": "publication-titles",
                    "relativePath": "outputs/titles.json"
                }]
            }),
        )
        .expect("execution result");
        fs::write(
            workspace.join("outputs/titles.json"),
            serde_json::to_vec(&json!({ "titles": ["Candidate title"] })).unwrap(),
        )
        .expect("title artifact");

        let draft = build_publication_title_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect("valid titles");
        let plan = TaskOutputPlan {
            content_hash: "sha256:plan".to_owned(),
            task_id: "task:title-target".to_owned(),
            attempt_id: "attempt:1".to_owned(),
            execution_generation: 1,
            handler_key: "handler.publication.title-generation".to_owned(),
            execution_bundle_hash: "sha256:bundle".to_owned(),
            execution_unit: json!({
                "kind": "task",
                "leaderTaskId": "task:title-target",
                "taskIds": ["task:title-target"],
                "taskType": "generate_publication_titles"
            }),
            actions: draft.actions,
        };

        let applied =
            apply_task_output_plan(&paths, "unused-for-title-output", &plan).expect("apply plan");
        let asset_ref = applied["artifacts"][0]["assetRef"]
            .as_str()
            .expect("asset ref");
        assert!(asset_ref.contains(&format!(
            "projects/{}/content/asset/",
            bootstrap.project.id
        )));
    }
}

#[cfg(test)]
mod wiki_output_discovery_tests {
    use super::*;

    fn wiki_task() -> Value {
        json!({
            "id": "task:wiki-output",
            "type": "ingest_markdown_into_wiki",
            "projectId": "project:wiki-output",
            "source": {
                "wikiSpaceId": "wiki:custom"
            }
        })
    }

    fn test_paths(temp: &tempfile::TempDir) -> MyOpenPanelsPaths {
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("wiki-output-test"),
        )
        .expect("paths")
    }

    #[test]
    fn wiki_runtime_discovers_arbitrary_markdown_structure_and_owns_metadata() {
        let temp = tempfile::tempdir().expect("temp");
        let paths = test_paths(&temp);
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs/wiki/knowledge/maps")).expect("outputs");
        fs::write(
            workspace.join("outputs/wiki/knowledge/maps/start-here.md"),
            "# Start here\n",
        )
        .expect("nested page");
        fs::write(workspace.join("outputs/wiki/home.md"), "# Home\n").expect("home page");
        write_test_result(
            &workspace,
            &json!({
                "outcome": "changed",
                "disposition": "included",
                "reasonCode": null,
                "summary": "Custom Wiki structure",
                "changedPaths": ["stale-agent-value.md"],
                "artifacts": ["stale-agent-value.md"]
            }),
        )
        .expect("execution result");

        let draft = build_wiki_output_plan(
            &paths,
            &wiki_task(),
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect("runtime-discovered Wiki outputs");

        assert_eq!(
            draft.result["changedPaths"],
            json!(["home.md", "knowledge/maps/start-here.md"])
        );
        assert_eq!(
            draft.result["artifacts"],
            json!([
                {
                    "role": "wiki-page",
                    "relativePath": "outputs/wiki/home.md",
                    "logicalPath": "home.md"
                },
                {
                    "role": "wiki-page",
                    "relativePath": "outputs/wiki/knowledge/maps/start-here.md",
                    "logicalPath": "knowledge/maps/start-here.md"
                }
            ])
        );
        assert_eq!(draft.actions.len(), 2);
    }

    #[test]
    fn wiki_runtime_rejects_non_markdown_outputs_and_no_change_with_pages() {
        let temp = tempfile::tempdir().expect("temp");
        let paths = test_paths(&temp);
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs/wiki/custom")).expect("outputs");
        fs::write(workspace.join("outputs/wiki/custom/index.json"), "{}").expect("json output");
        write_test_result(
            &workspace,
            &json!({
                "outcome": "changed",
                "disposition": "included",
                "reasonCode": null,
                "summary": "Invalid non-Markdown output"
            }),
        )
        .expect("execution result");

        let error = build_wiki_output_plan(
            &paths,
            &wiki_task(),
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect_err("non-Markdown Wiki output must fail");
        assert_eq!(error.code(), Some("invalid_output"));
        assert!(error.message().contains("must be Markdown files"));

        fs::remove_file(workspace.join("outputs/wiki/custom/index.json")).expect("remove json");
        fs::write(workspace.join("outputs/wiki/custom/index.md"), "# Index\n")
            .expect("Markdown output");
        write_test_result(
            &workspace,
            &json!({
                "outcome": "no_change",
                "disposition": "already_covered",
                "reasonCode": null,
                "summary": "Contradictory no-change output"
            }),
        )
        .expect("execution result");
        let error = build_wiki_output_plan(
            &paths,
            &wiki_task(),
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect_err("no_change with Markdown pages must fail");
        assert_eq!(error.code(), Some("invalid_output"));
        assert!(error
            .message()
            .contains("no_change output cannot include changed paths"));
    }
}
