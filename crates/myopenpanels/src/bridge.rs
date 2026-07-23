include!("bridge/worker.rs");
include!("bridge/task_handlers.rs");
include!("bridge/execution.rs");
include!("bridge/document_prompts.rs");
include!("bridge/writing_wiki_prompts.rs");
include!("bridge/result_validation.rs");
include!("bridge/conversion_output.rs");
include!("bridge/finalization.rs");
include!("bridge/status_process.rs");

#[cfg(test)]
mod tests {
    use super::*;

    fn active_wiki_space_id(paths: &crate::paths::MyOpenPanelsPaths) -> String {
        crate::wiki::wiki_context(paths).expect("wiki context")["state"]["activeWikiSpaceId"]
            .as_str()
            .expect("active Wiki Space")
            .to_owned()
    }

    include!("bridge/tests/writing.rs");
    include!("bridge/tests/conversion_generation.rs");
    include!("bridge/tests/distillation.rs");
    include!("bridge/tests/wiki.rs");
    include!("bridge/tests/skills_process.rs");
}
