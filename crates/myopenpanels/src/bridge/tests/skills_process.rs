    #[test]
    fn materialized_skill_package_preserves_nested_text_files() {
        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("skill");
        let destination = temp.path().join("execution/skills/custom");
        fs::create_dir_all(source.join("arbitrary/layout")).expect("skill directories");
        fs::write(source.join("SKILL.md"), "# Custom Skill\n").expect("skill file");
        fs::write(
            source.join("arbitrary/layout/rules.txt"),
            "Use any page layout.\n",
        )
        .expect("nested text");

        let files = materialize_skill_tree(&source, &destination).expect("materialize skill");
        assert!(destination.join("SKILL.md").is_file());
        assert!(destination.join("arbitrary/layout/rules.txt").is_file());
        assert!(render_skill_package(&files).contains("arbitrary/layout/rules.txt"));
    }

    #[test]
    fn materialized_skill_package_rejects_binary_files() {
        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("skill");
        let destination = temp.path().join("execution/skills/custom");
        fs::create_dir_all(&source).expect("skill directory");
        fs::write(source.join("SKILL.md"), "# Custom Skill\n").expect("skill file");
        fs::write(source.join("asset.bin"), [0xff, 0x00, 0x81]).expect("binary asset");

        let error = materialize_skill_tree(&source, &destination)
            .expect_err("binary Skill files must fail");

        assert_eq!(error.code(), Some("invalid_skill_package"));
        assert!(!destination.join("asset.bin").exists());
    }

    #[cfg(unix)]
    #[test]
    fn materialized_skill_package_does_not_follow_external_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp");
        let source = temp.path().join("skill");
        let destination = temp.path().join("execution/skills/custom");
        fs::create_dir_all(&source).expect("skill directory");
        fs::write(source.join("SKILL.md"), "# Custom Skill\n").expect("skill file");
        fs::write(temp.path().join("secret.txt"), "outside\n").expect("external file");
        symlink(temp.path().join("secret.txt"), source.join("external.txt")).expect("symlink");

        let files = materialize_skill_tree(&source, &destination).expect("materialize skill");

        assert!(!destination.join("external.txt").exists());
        assert!(!files
            .iter()
            .any(|file| file.relative_path == "external.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn task_command_uses_attempt_workspace_and_cleans_it_after_exit() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-workspace-test"),
        )
        .expect("paths");

        let result = run_task_command(
            &paths,
            "printf '%s|%s|%s|%s' \"$MYOPENPANELS_TASK_ATTEMPT_ID\" \"$MYOPENPANELS_EXECUTION_GENERATION\" \"$PWD\" \"${MYOPENPANELS_STORAGE_DIR-unset}\"; touch \"$MYOPENPANELS_EXECUTION_WORKSPACE/output.md\"",
            1_000,
            &json!({
                "id": "task:workspace",
                "queue": "wiki",
                "capability": "wiki.ingestMarkdown",
            }),
            "target:test",
            "lease:test",
            false,
            Some("attempt:test"),
            Some(7),
            None,
            None,
            None,
        )
        .expect("command");

        let stdout = result["stdout"].as_str().expect("stdout");
        assert!(stdout.starts_with("attempt:test|7|"));
        assert!(stdout.ends_with("|unset"));
        assert!(stdout.contains(
            &storage
                .join("executions/task_workspace/7-attempt_test")
                .display()
                .to_string()
        ));
        assert!(!storage.join("executions").join("task-workspace").exists());
    }

    #[cfg(unix)]
    #[test]
    fn task_command_terminates_its_process_group_when_shutdown_is_requested() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-shutdown-test"),
        )
        .expect("paths");
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_signal = shutdown.clone();
        let signal = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            shutdown_signal.store(true, Ordering::Release);
        });
        let started = Instant::now();

        let result = run_task_command(
            &paths,
            "sleep 30",
            60_000,
            &json!({
                "id": "task:shutdown",
                "queue": "wiki",
                "capability": "wiki.ingestMarkdown",
            }),
            "target:test",
            "lease:test",
            false,
            Some("attempt:test"),
            Some(1),
            None,
            None,
            Some(&shutdown),
        )
        .expect("command");
        signal.join().expect("shutdown signal");

        assert_eq!(result["interrupted"], true);
        assert_eq!(result["success"], false);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn bridge_failure_prefers_the_final_codex_turn_error() {
        let result = json!({
            "stdout": concat!(
                "{\"type\":\"error\",\"message\":\"Reconnecting...\"}\n",
                "{\"type\":\"turn.failed\",\"error\":{\"message\":\"stream disconnected before completion: Connection refused\"}}\n",
                "{\"type\":\"error\",\"message\":\"analytics shutdown failed\"}\n"
            ),
            "stderr": "earlier model cache warning",
        });

        assert_eq!(
            bridge_failure_message(&result),
            "stream disconnected before completion: Connection refused"
        );
    }

    #[test]
    fn bridge_failure_uses_the_end_of_stderr_without_structured_error() {
        let result = json!({
            "stdout": "",
            "stderr": "early warning\nfatal connection error\n",
        });

        assert_eq!(
            bridge_failure_message(&result),
            "early warning\nfatal connection error"
        );
    }

    #[test]
    fn reads_the_final_codex_agent_message() {
        let stdout = concat!(
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"earlier\"}}\n",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"sandbox_apply: Operation not permitted\"}}\n"
        );

        assert_eq!(
            final_agent_message(stdout).as_deref(),
            Some("sandbox_apply: Operation not permitted")
        );
    }
