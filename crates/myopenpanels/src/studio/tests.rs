#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_myopenpanels_paths;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn paths_for(project_dir: &Path, storage_dir: &Path, context_id: &str) -> MyOpenPanelsPaths {
        resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some(context_id),
        )
        .expect("paths")
    }

    #[test]
    fn browser_commands_cover_supported_platforms() {
        let url = "http://127.0.0.1:43217/wiki?tab=source";
        assert_eq!(
            browser_command(BrowserPlatform::Macos, url),
            ("open", vec![url.to_owned()])
        );
        assert_eq!(
            browser_command(BrowserPlatform::Windows, url),
            (
                "cmd",
                vec![
                    "/c".to_owned(),
                    "start".to_owned(),
                    String::new(),
                    url.to_owned(),
                ]
            )
        );
        assert_eq!(
            browser_command(BrowserPlatform::Other, url),
            ("xdg-open", vec![url.to_owned()])
        );
    }

    #[test]
    fn browser_open_succeeds_only_when_launcher_succeeds() {
        let url = "http://127.0.0.1:43217";
        let result = open_browser_with(url, BrowserPlatform::Macos, |program, args| {
            assert_eq!(program, "open");
            assert_eq!(args, &[url.to_owned()]);
            Ok(true)
        });

        assert!(result.is_ok());
    }

    #[test]
    fn browser_open_reports_non_zero_launcher_status() {
        let url = "http://127.0.0.1:43217";
        let error = open_browser_with(url, BrowserPlatform::Other, |_, _| Ok(false))
            .expect_err("non-zero launcher status");

        assert_eq!(error.code(), Some("browser_open_failed"));
        assert!(error.retryable());
        assert!(error.message().contains("non-zero status"));
        assert!(error.recovery().unwrap().contains(url));
    }

    #[test]
    fn browser_open_reports_launcher_start_failure() {
        let url = "http://127.0.0.1:43217";
        let error = open_browser_with(url, BrowserPlatform::Windows, |_, _| {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "launcher missing",
            ))
        })
        .expect_err("missing launcher");

        assert_eq!(error.code(), Some("browser_open_failed"));
        assert!(error.message().contains("launcher missing"));
        assert!(error.recovery().unwrap().contains(url));
    }

    fn fake_studio_server(request_count: usize) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("local addr").port();
        let handle = thread::spawn(move || {
            let body = format!(
                "{{\"ok\":true,\"version\":\"{}\"}}",
                env!("CARGO_PKG_VERSION")
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
            for _ in 0..request_count {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                stream.write_all(response.as_bytes()).expect("response");
            }
        });
        (port, handle)
    }

    #[test]
    fn wait_for_studio_does_not_hang_on_unresponsive_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("local addr").port();
        let server = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                thread::sleep(Duration::from_secs(2));
            }
        });

        let started = Instant::now();
        let result = wait_for_studio(
            &format!("http://127.0.0.1:{port}"),
            Duration::from_millis(50),
        );

        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().expect("server thread");
    }

    #[test]
    fn studio_versions_are_compared_before_reuse() {
        assert_eq!(
            compare_studio_version(Some(env!("CARGO_PKG_VERSION"))).expect("current"),
            StudioVersionRelation::Current
        );
        assert_eq!(
            compare_studio_version(Some("0.2.2")).expect("older"),
            StudioVersionRelation::Older
        );
        assert_eq!(
            compare_studio_version(Some("99.0.0")).expect("newer"),
            StudioVersionRelation::Newer
        );
        assert_eq!(
            compare_studio_version(None).expect_err("missing version").code(),
            Some("studio_version_mismatch")
        );
        let error = compare_studio_version(Some("not-semver")).expect_err("invalid");
        assert_eq!(error.code(), Some("studio_version_mismatch"));
    }

    #[test]
    fn transition_lock_serializes_agents_sharing_storage() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_a = temp.path().join("project-a");
        let project_b = temp.path().join("project-b");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_a).expect("project a");
        fs::create_dir_all(&project_b).expect("project b");
        let paths_a = paths_for(&project_a, &storage_dir, "agent-a");
        let paths_b = paths_for(&project_b, &storage_dir, "agent-b");
        let first = acquire_studio_transition_lock(&paths_a).expect("first lock");
        let started = Instant::now();
        let waiter = thread::spawn(move || {
            let _second = acquire_studio_transition_lock(&paths_b).expect("second lock");
            started.elapsed()
        });

        thread::sleep(Duration::from_millis(150));
        drop(first);
        let waited = waiter.join().expect("waiter");

        assert!(waited >= Duration::from_millis(100));
    }

    fn studio_session(paths: &MyOpenPanelsPaths, port: u16) -> StudioSession {
        let server_url = format!("http://127.0.0.1:{port}");
        StudioSession {
            system_browser_url: Some(server_url.clone()),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: paths.context_dir.join("studio.log").display().to_string(),
            pid: std::process::id(),
            port,
            server_url,
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: paths.storage_dir.display().to_string(),
        }
    }

    #[test]
    fn start_reuses_the_running_storage_singleton() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let (port, server) = fake_studio_server(2);
        let owner_session = studio_session(&owner_paths, port);
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = start_studio(
            &borrower_paths,
            StudioStartOptions {
                host: "127.0.0.1".to_owned(),
                static_dir: None,
            },
        )
        .expect("start");

        assert!(result.reused_existing);
        assert_eq!(result.lifecycle, StudioLifecycle::Reused);
        assert_eq!(result.server_version, env!("CARGO_PKG_VERSION"));
        assert!(!result.browser_refresh_required);
        assert_eq!(result.session.server_url, owner_session.server_url);
        assert_eq!(result.session.storage_dir, owner_session.storage_dir);
        assert!(studio_session_path(&borrower_paths).exists());
        server.join().expect("server thread");
    }

    #[test]
    fn start_reuses_the_owner_when_health_and_process_checks_are_unavailable() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let port = 41_003;
        let owner_session = studio_session(&owner_paths, port);
        write_studio_session(&owner_paths, &owner_session).expect("owner session");
        let _owner_lock =
            acquire_studio_owner_lock(&owner_paths, port).expect("owner lock");

        let result = start_studio(
            &borrower_paths,
            StudioStartOptions {
                host: "127.0.0.1".to_owned(),
                static_dir: None,
            },
        )
        .expect("reuse owner");

        assert!(result.reused_existing);
        assert_eq!(result.lifecycle, StudioLifecycle::Reused);
        assert_eq!(result.session.pid, owner_session.pid);
        assert_eq!(result.session.port, owner_session.port);
        assert_eq!(result.server_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn owner_lock_rejects_a_second_server_for_the_same_storage() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let _owner_lock =
            acquire_studio_owner_lock(&owner_paths, 41_001).expect("first owner lock");

        let error = match acquire_studio_owner_lock(&borrower_paths, 41_002) {
            Ok(_) => panic!("second owner lock unexpectedly succeeded"),
            Err(error) => error,
        };

        assert_eq!(error.code(), Some("studio_already_running"));
    }

    #[test]
    fn owner_lock_survives_storage_directory_replacement() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        let moved_storage_dir = temp.path().join("moved-myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let owner_lock =
            acquire_studio_owner_lock(&owner_paths, 41_001).expect("first owner lock");
        fs::rename(&storage_dir, &moved_storage_dir).expect("move storage");
        fs::create_dir_all(&storage_dir).expect("replacement storage");

        let error = match acquire_studio_owner_lock(&borrower_paths, 41_002) {
            Ok(_) => panic!("replacement storage acquired a second owner lock"),
            Err(error) => error,
        };

        assert_eq!(error.code(), Some("studio_already_running"));
        drop(owner_lock);
    }

    #[test]
    fn reuse_returns_the_storage_singleton_for_a_different_project_and_context() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let other_project_dir = temp.path().join("other-project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        fs::create_dir_all(&other_project_dir).expect("other project dir");
        let owner_paths = paths_for(&other_project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let (port, server) = fake_studio_server(1);
        let owner_session = studio_session(&owner_paths, port);
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = reuse_existing_studio(&borrower_paths)
            .expect("reuse")
            .expect("singleton session");

        assert_eq!(result.pid, owner_session.pid);
        assert_eq!(result.server_url, owner_session.server_url);
        assert_eq!(result.storage_dir, owner_session.storage_dir);
        server.join().expect("server thread");
    }

    #[test]
    fn reuse_removes_the_stale_storage_instance() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let mut owner_session = studio_session(&owner_paths, 65_000);
        owner_session.pid = 0;
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = reuse_existing_studio(&borrower_paths).expect("reuse");

        assert!(result.is_none());
        assert!(!studio_session_path(&owner_paths).exists());
    }

    #[test]
    fn current_studio_resolution_returns_the_storage_singleton() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let caller_paths = paths_for(&project_dir, &storage_dir, "caller");
        let (port, server) = fake_studio_server(1);
        let caller_session = studio_session(&caller_paths, port);
        write_studio_session(&caller_paths, &caller_session).expect("caller session");

        let resolved = resolve_current_studio_session(&caller_paths)
            .expect("resolution")
            .expect("session");

        assert_eq!(resolved.server_url, caller_session.server_url);
        server.join().expect("server thread");
    }

    #[test]
    fn stop_from_another_context_stops_the_storage_singleton() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let mut owner_session = studio_session(&owner_paths, 65_000);
        owner_session.pid = 0;
        write_studio_session(&borrower_paths, &owner_session).expect("borrowed session");

        let result = stop_studio_session(&borrower_paths).expect("stop");

        assert!(result.stopped);
        assert!(!studio_session_path(&borrower_paths).exists());
    }

    #[test]
    fn stop_removes_the_storage_instance() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let mut owner_session = studio_session(&owner_paths, 65_000);
        owner_session.pid = 0;
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = stop_studio_session(&owner_paths).expect("stop");

        assert!(result.stopped);
        assert!(!studio_session_path(&owner_paths).exists());
    }

    #[test]
    fn stop_then_start_prefers_the_previous_port() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = paths_for(&project_dir, &storage_dir, "owner");
        let port = find_open_port("127.0.0.1").expect("open port");
        let mut session = studio_session(&paths, port);
        session.pid = 0;
        write_studio_session(&paths, &session).expect("session");

        stop_studio_session(&paths).expect("stop");

        assert_eq!(
            find_studio_port(&paths, "127.0.0.1").expect("preferred port"),
            port
        );
    }

    #[test]
    fn start_falls_back_when_the_previous_port_is_busy() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = paths_for(&project_dir, &storage_dir, "owner");
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let busy_port = listener.local_addr().expect("local addr").port();
        write_studio_port_preference(&paths, busy_port).expect("port preference");

        let selected = find_studio_port(&paths, "127.0.0.1").expect("fallback port");

        assert_ne!(selected, busy_port);
    }
}
