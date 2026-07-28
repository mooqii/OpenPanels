#[test]
fn publishing_skills_select_their_platform_route() {
    let xiaohongshu =
        publishing_target_for_skill(&publishing_skill_listing(XIAOHONGSHU_TASK_TYPE))
            .expect("Xiaohongshu target");
    assert_eq!(xiaohongshu.platform, "xiaohongshu");
    assert_eq!(
        publishing_task_capability(xiaohongshu.task_type).expect("Xiaohongshu capability"),
        "release.xiaohongshu"
    );

    let wechat =
        publishing_target_for_skill(&publishing_skill_listing(WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE))
            .expect("WeChat target");
    assert_eq!(wechat.platform, "wechat_official_account");
    assert_eq!(
        publishing_task_capability(wechat.task_type).expect("WeChat capability"),
        "release.wechat_official_account"
    );

    let x = publishing_target_for_skill(&publishing_skill_listing(X_TASK_TYPE)).expect("X target");
    assert_eq!(x.platform, "x");
    assert_eq!(
        publishing_task_capability(x.task_type).expect("X capability"),
        "release.x"
    );

    let reddit = publishing_target_for_skill(&publishing_skill_listing(REDDIT_TASK_TYPE))
        .expect("Reddit target");
    assert_eq!(reddit.platform, "reddit");
    assert_eq!(
        publishing_task_capability(reddit.task_type).expect("Reddit capability"),
        "release.reddit"
    );
}
