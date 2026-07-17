#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_myopenpanels_paths;

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
    fn agent_target_registration_rejects_removed_endpoint_field() {
        let body = serde_json::from_value::<AgentTargetRegistrationBody>(json!({
            "name": "poll-worker",
            "transport": "poll",
            "endpoint": "http://localhost/wake",
            "capabilities": ["*"]
        }));
        assert!(body.is_err());
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
