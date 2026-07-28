#[test]
fn xiaohongshu_prompt_accepts_text_without_media() {
    let temp = tempfile::tempdir().expect("temp");
    let project = temp.path().join("project");
    let storage = temp.path().join("storage");
    let workspace = temp.path().join("workspace");
    let inputs = workspace.join("inputs");
    let skill = inputs.join("skill");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&skill).expect("skill directory");
    let title_path = inputs.join("title.txt");
    let body_path = inputs.join("body.txt");
    let tags_path = inputs.join("tags.json");
    fs::write(&title_path, "").expect("title");
    fs::write(&body_path, "Body only").expect("body");
    fs::write(&tags_path, r#"["writing","AI"]"#).expect("tags");
    fs::write(skill.join("SKILL.md"), "# Publishing\n").expect("skill");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project.to_str().unwrap()),
        Some(storage.to_str().unwrap()),
        Some("text-only-publishing-prompt-test"),
    )
    .expect("paths");
    let task = json!({
        "id": "task:text-only",
        "queue": "release",
        "type": "release_xiaohongshu",
        "capability": "release.xiaohongshu",
        "input": {
            "releaseId": "release:1",
            "attemptId": "attempt:1"
        },
        "executionInputs": {
            "release": {
                "titleFilePath": title_path,
                "bodyFilePath": body_path,
                "tagsFilePath": tags_path,
                "media": [],
                "skillDirectory": skill
            }
        }
    });

    let prompt = format!(
        "{}\n\n{}",
        render_task_platform_contract(&task).expect("Publishing Platform Contract"),
        build_xiaohongshu_publishing_prompt(&paths, &task, &workspace)
            .expect("text-only publishing prompt")
    );
    assert!(prompt.contains("Ordered media files:\n(none)"));
    assert!(prompt.contains("Publishing mode: `text-only note`"));
    assert!(prompt.contains("The declared mode is `text-only note`"));
    assert!(prompt.contains("Add every non-empty tag exactly once"));
    assert!(prompt.contains("tags.json"));
    assert!(prompt.contains("# Publishing Panel Contract"));
    assert!(prompt.contains("# Execute A Publishing Request"));
    assert!(prompt.contains("Task id: `task:text-only`"));
    assert!(prompt.contains(&format!(
        "Result file: `{}`",
        workspace.join(EXECUTION_RESULT_FILE).display()
    )));
    assert!(prompt
        .contains("release checkpoint --task-id task:text-only --phase prepared --format json"));

    let mut image_task = task.clone();
    image_task["id"] = json!("task:image");
    image_task["executionInputs"]["release"]["media"] = json!([{
        "filePath": inputs.join("cover.png"),
        "fileName": "cover.png",
        "mimeType": "image/png"
    }]);
    let image_prompt = build_xiaohongshu_publishing_prompt(&paths, &image_task, &workspace)
        .expect("image publishing prompt");
    assert!(image_prompt.contains("Publishing mode: `image note`"));
    assert!(image_prompt.contains("contains no video"));
    assert!(image_prompt.contains("upload all numbered images in one selection"));
    assert!(image_prompt.contains("(image/png, primary cover)"));

    let mut video_task = task.clone();
    video_task["id"] = json!("task:video");
    video_task["executionInputs"]["release"]["media"] = json!([
        {
            "filePath": inputs.join("cover.png"),
            "fileName": "cover.png",
            "mimeType": "image/png"
        },
        {
            "filePath": inputs.join("clip.mp4"),
            "fileName": "clip.mp4",
            "mimeType": "video/mp4"
        }
    ]);
    let video_prompt = build_xiaohongshu_publishing_prompt(&paths, &video_task, &workspace)
        .expect("video publishing prompt");
    assert!(video_prompt.contains("Publishing mode: `video note`"));
    assert!(video_prompt.contains("at least one supplied cover media item is a video"));
    assert!(video_prompt.contains("upload the first numbered video"));
    assert!(video_prompt.contains("never fall back to an image note"));
    assert!(video_prompt.contains("(video/mp4)"));

    let mut wechat_task = task;
    wechat_task["type"] = json!("release_wechat_official_account");
    wechat_task["capability"] = json!("release.wechat_official_account");
    let prompt = format!(
        "{}\n\n{}",
        render_task_platform_contract(&wechat_task).expect("Publishing Platform Contract"),
        build_wechat_official_account_publishing_prompt(&paths, &wechat_task, &workspace)
            .expect("WeChat draft publishing prompt")
    );
    assert!(prompt.contains("Do not open or automate `mp.weixin.qq.com`"));
    assert!(prompt.contains(
        "release wechat draft --task-id task:text-only --format json"
    ));
    assert!(prompt.contains("calls the documented server-side WeChat draft API"));
    assert!(prompt.contains("wechat_topics_unsupported"));
    assert!(prompt.contains("\"platform\": \"wechat_official_account\""));

    let mut x_task = wechat_task.clone();
    x_task["type"] = json!("release_x");
    x_task["capability"] = json!("release.x");
    let prompt =
        build_x_publishing_prompt(&paths, &x_task, &workspace).expect("X publishing prompt");
    assert!(prompt.contains("Standard posts allow 280 characters"));
    assert!(prompt.contains("\"platform\": \"x\""));

    let mut reddit_task = wechat_task;
    reddit_task["type"] = json!("release_reddit");
    reddit_task["capability"] = json!("release.reddit");
    let prompt = build_reddit_publishing_prompt(&paths, &reddit_task, &workspace)
        .expect("Reddit publishing prompt");
    assert!(prompt.contains("Exactly one tag must have `r/community` form"));
    assert!(prompt.contains("reddit_destination_required"));
    assert!(prompt.contains("\"platform\": \"reddit\""));
}
