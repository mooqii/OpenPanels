include!("bridge/worker.rs");
include!("bridge/task_handlers.rs");
include!("bridge/execution.rs");
include!("bridge/document_prompts.rs");
include!("bridge/writing_wiki_prompts.rs");
include!("bridge/result_validation.rs");
include!("bridge/finalization.rs");
include!("bridge/status_process.rs");

#[cfg(test)]
mod tests {
    use super::*;
    include!("bridge/tests/writing.rs");
    include!("bridge/tests/conversion_generation.rs");
    include!("bridge/tests/refinement.rs");
    include!("bridge/tests/wiki.rs");
    include!("bridge/tests/skills_process.rs");
}
