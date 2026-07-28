pub const X_TASK_TYPE: &str = "release_x";
pub const REDDIT_TASK_TYPE: &str = "release_reddit";

const X_TARGET: PublishingTarget = PublishingTarget {
    platform: "x",
    task_type: X_TASK_TYPE,
};

const REDDIT_TARGET: PublishingTarget = PublishingTarget {
    platform: "reddit",
    task_type: REDDIT_TASK_TYPE,
};
