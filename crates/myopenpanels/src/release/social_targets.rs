pub const BILIBILI_TASK_TYPE: &str = "release_bilibili";
pub const X_TASK_TYPE: &str = "release_x";
pub const REDDIT_TASK_TYPE: &str = "release_reddit";
pub const V2EX_TASK_TYPE: &str = "release_v2ex";

const BILIBILI_TARGET: PublishingTarget = PublishingTarget {
    platform: "bilibili",
    task_type: BILIBILI_TASK_TYPE,
};

const X_TARGET: PublishingTarget = PublishingTarget {
    platform: "x",
    task_type: X_TASK_TYPE,
};

const REDDIT_TARGET: PublishingTarget = PublishingTarget {
    platform: "reddit",
    task_type: REDDIT_TASK_TYPE,
};

const V2EX_TARGET: PublishingTarget = PublishingTarget {
    platform: "v2ex",
    task_type: V2EX_TASK_TYPE,
};
