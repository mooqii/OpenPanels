#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{resolve_myopenpanels_paths, sanitize_path_part};
    use axum::http::Request;
    use tower::ServiceExt;

    #[test]
    fn update_status_query_can_bypass_cached_checks() {
        assert!(!UpdateStatusQuery::default().bypass_cache());
        assert!(UpdateStatusQuery {
            refresh: Some("1".to_owned()),
            ..UpdateStatusQuery::default()
        }
        .bypass_cache());
        assert!(UpdateStatusQuery {
            force: Some("true".to_owned()),
            ..UpdateStatusQuery::default()
        }
        .bypass_cache());
    }

    #[test]
    fn build_info_advertises_the_agent_cli_for_its_runtime() {
        let build_info = serde_json::to_value(current_build_info()).expect("build info");
        assert!(build_info["agentCli"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }

    #[tokio::test]
    async fn uploaded_assets_use_only_project_content_storage() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("asset-upload-storage"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let panel_id = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Typesetting)
            .expect("typesetting panel")
            .panel
            .id
            .clone();
        let router = build_router(
            "127.0.0.1".to_owned(),
            0,
            paths.clone(),
            None,
            current_build_info(),
        );
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/assets")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "dataUrl": "data:image/png;base64,Y292ZXI=",
                            "fileName": "cover.png",
                            "mimeType": "image/png",
                            "originPanelId": panel_id,
                        })
                        .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload: Value = serde_json::from_slice(&body).expect("asset payload");
        let asset_ref = payload["assetRef"].as_str().expect("asset ref");
        assert!(asset_ref.starts_with(&format!(
            "projects/{}/content/asset/",
            bootstrap.project.id
        )));
        assert!(storage_dir.join(asset_ref).is_file());
        assert!(!storage_dir
            .join("projects")
            .join(sanitize_path_part(&bootstrap.project.id))
            .join("panels")
            .exists());
    }

    #[test]
    fn browser_path_only_accepts_local_relative_urls() {
        assert_eq!(
            safe_browser_path(Some("/wiki?tab=source#top")),
            "/wiki?tab=source#top"
        );
        assert_eq!(safe_browser_path(Some("https://example.com")), "/");
        assert_eq!(safe_browser_path(Some("//example.com")), "/");
        assert_eq!(safe_browser_path(Some("/\r\nexample.com")), "/");
    }

    #[tokio::test]
    async fn studio_browser_endpoint_reports_successful_system_open() {
        let url = "http://127.0.0.1:43217/wiki";
        let response = studio_open_browser_response(url, |opened_url| {
            assert_eq!(opened_url, url);
            Ok(())
        });

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload = serde_json::from_slice::<Value>(&body).expect("json response");
        assert_eq!(payload["opened"], true);
        assert_eq!(payload["openTarget"], "system_browser");
        assert_eq!(payload["url"], url);
    }

    #[tokio::test]
    async fn studio_browser_endpoint_propagates_launcher_failure() {
        let url = "http://127.0.0.1:43217";
        let response = studio_open_browser_response(url, |_| {
            Err(CliError::with_recovery(
                "browser_open_failed",
                "browser launcher failed",
                true,
                format!("Open {url} manually."),
            ))
        });

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload = serde_json::from_slice::<Value>(&body).expect("json response");
        assert_eq!(payload["error"], "browser launcher failed");
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let state = Arc::new(AppState {
            build_info: current_build_info(),
            host: "127.0.0.1".to_owned(),
            paths,
            port: 0,
            project_event_slots: Arc::new(Semaphore::new(MAX_PROJECT_EVENT_STREAMS)),
            static_dir: None,
        });

        let response = api_health(State(state.clone())).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );

        let response = api_bootstrap(
            State(state),
            Query(BootstrapQuery {
                panel_id: None,
                panel_kind: None,
                project_id: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }

    #[tokio::test]
    async fn project_event_stream_stops_polling_after_client_disconnect() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("event-disconnect"),
        )
        .expect("paths");
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let (sender, receiver) =
            mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();
        drop(receiver);
        let permit = Arc::new(Semaphore::new(1))
            .acquire_owned()
            .await
            .expect("event stream permit");

        assert!(
            tokio::time::timeout(
                Duration::from_millis(100),
                run_project_event_stream(paths, None, sender, permit, Duration::from_millis(10))
            )
            .await
            .is_ok(),
            "disconnected event stream kept polling"
        );
    }

    #[tokio::test]
    async fn project_event_streams_leave_capacity_for_regular_requests() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("event-capacity"),
        )
        .expect("paths");
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let state = Arc::new(AppState {
            build_info: current_build_info(),
            host: "127.0.0.1".to_owned(),
            paths,
            port: 0,
            project_event_slots: Arc::new(Semaphore::new(1)),
            static_dir: None,
        });
        let query = || Query(ProjectEventsQuery { project_id: None });

        let first = api_project_events(State(state.clone()), query()).await;
        assert_eq!(first.status(), StatusCode::OK);
        let saturated = api_project_events(State(state.clone()), query()).await;
        assert_eq!(saturated.status(), StatusCode::NO_CONTENT);

        drop(first);
        tokio::time::sleep(Duration::from_millis(
            PROJECT_EVENT_POLL_INTERVAL_MS * 2,
        ))
        .await;

        let recovered = api_project_events(State(state), query()).await;
        assert_eq!(recovered.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn skill_management_endpoints_list_and_protect_presets() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("skills-api"),
        )
        .expect("paths");
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let state = Arc::new(AppState {
            build_info: current_build_info(),
            host: "127.0.0.1".to_owned(),
            paths,
            port: 0,
            project_event_slots: Arc::new(Semaphore::new(MAX_PROJECT_EVENT_STREAMS)),
            static_dir: None,
        });

        let response = api_skills(State(state.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload = serde_json::from_slice::<Value>(&body).expect("json response");
        assert!(payload["systemSkills"].as_array().is_some_and(|value| !value.is_empty()));

        let response = api_recommended_skills(State(state.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("recommended response body");
        let payload =
            serde_json::from_slice::<Value>(&body).expect("recommended json response");
        assert!(payload.get("schemaVersion").is_none());
        assert_eq!(
            payload["skills"][0]["id"],
            json!("guizang-social-card-skill")
        );
        assert_eq!(
            payload["skills"][0]["moduleKinds"],
            json!(["publication-cover"])
        );

        let response = api_install_recommended_skill(
            State(state.clone()),
            Path("missing-recommendation".to_owned()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = api_device_skills(State(state.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("device response body");
        let payload = serde_json::from_slice::<Value>(&body).expect("device json response");
        assert_eq!(
            payload["availableModuleKinds"],
            json!([
                "wiki-update",
                "writing",
                "writing-distillation",
                "publication-cover",
                "publication-title",
                "publication-layout",
                "release"
            ])
        );

        let response = api_install_device_skill(
            State(state.clone()),
            Json(DeviceSkillInstallBody {
                location_path: "/missing".to_owned(),
                module_kind: "unknown".to_owned(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = api_import_skill(
            State(state.clone()),
            Json(SkillImportBody {
                source_type: "folder".to_owned(),
                module_kind: "writing".to_owned(),
                replace_existing: false,
                url: String::new(),
                archive_base64: String::new(),
                skills: Vec::new(),
                files: vec![crate::agent::SkillImportFile {
                    path: "api-import/SKILL.md".to_owned(),
                    content_base64: base64::engine::general_purpose::STANDARD.encode(
                        "---\nname: api-import\ndescription: Imported over the API.\n---\n\nWrite directly.\n",
                    ),
                }],
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = api_check_skill_updates(State(state.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("update response body");
        let payload = serde_json::from_slice::<Value>(&body).expect("update json response");
        assert!(payload["skills"].as_array().unwrap().iter().any(|skill| {
            skill["status"] == "unmanaged" && skill["skillId"].as_str().is_some()
        }));

        let imported_skill_id = crate::agent::skill_update_ids(&state.paths)
            .unwrap()
            .into_iter()
            .find(|skill_id| skill_id.starts_with("custom-"))
            .expect("imported skill id");
        let response = api_update_skill(
            State(state.clone()),
            Path(imported_skill_id),
            Json(SkillUpdateBody { force: false }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = api_write_skill_file(
            State(state),
            Path("writing-default".to_owned()),
            Json(ManagedSkillFileBody {
                path: "SKILL.md".to_owned(),
                content: "not allowed".to_owned(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn studio_static_assets_use_release_cache_policy() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let state = AppState {
            build_info: current_build_info(),
            host: "127.0.0.1".to_owned(),
            paths,
            port: 0,
            project_event_slots: Arc::new(Semaphore::new(MAX_PROJECT_EVENT_STREAMS)),
            static_dir: None,
        };

        let index = serve_static_path(&state, "index.html");
        assert_eq!(
            index.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache, must-revalidate"
        );

        let asset = STUDIO_DIST
            .get_dir("assets")
            .and_then(|assets| assets.files().next())
            .expect("built studio asset");
        let asset_path = asset.path().to_str().expect("asset path");
        let response = serve_static_path(&state, asset_path);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
    }
}
