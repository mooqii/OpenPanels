use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use crate::selection::read_selection;
use crate::types::{PanelKind, ProjectBootstrap};
use crate::wiki::{read_agent_selection, selected_agent_skill_id, WIKI_QUERY_SKILL_ID};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

static AGENT_GUIDES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../agent-resources/guides");
static AGENT_SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../agent-resources/skills");
const WIKI_KNOWLEDGE_GUIDE_ID: &str = "wiki.knowledge-context";
const WIKI_GENERATED_DOCUMENTS_GUIDE_ID: &str = "wiki.generated-documents";
pub const MYOPENPANELS_SKILL_VERSION: &str = "1.0";
const MYOPENPANELS_SKILL_SOURCE: &str =
    "https://github.com/mooqii/OpenPanels/tree/main/skills/myopenpanels";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentGuideMetadata {
    pub applies_to: Vec<String>,
    pub id: String,
    pub load_when: Vec<String>,
    pub requires_capabilities: Vec<String>,
    pub source: String,
    pub task_types: Vec<String>,
    pub title: String,
    pub tokens: String,
}

#[derive(Debug, Clone)]
pub struct AgentGuide {
    pub metadata: AgentGuideMetadata,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct AgentGuideReadPayload {
    pub guide: AgentGuideMetadata,
    pub markdown: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillMetadata {
    pub applies_to: Vec<String>,
    pub description: String,
    pub id: String,
    pub load_when: Vec<String>,
    pub requires_capabilities: Vec<String>,
    pub source: String,
    pub task_types: Vec<String>,
    pub title: String,
    pub tokens: String,
}

#[derive(Debug, Clone)]
pub struct AgentSkill {
    pub metadata: AgentSkillMetadata,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillListing {
    pub skill: AgentSkillMetadata,
    pub local_dir: String,
    pub local_path: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillReadPayload {
    pub skill: AgentSkillMetadata,
    pub local_dir: String,
    pub local_path: String,
    pub markdown: String,
}

pub fn agent_bootstrap(paths: &OpenPanelsPaths, cli_version: &str) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let guides = list_agent_guides()?;
    let skills = list_agent_skills(paths)?;
    let selection = read_selection(paths, None, false).ok();
    let wiki_selection = read_agent_selection(paths).ok();
    let mut payload = agent_bootstrap_payload(
        &bootstrap,
        cli_version,
        &guides,
        &skills,
        selection.as_ref(),
        wiki_selection.as_ref(),
    );
    let applicable_guides = guides
        .iter()
        .filter(|guide| {
            guide
                .applies_to
                .iter()
                .any(|kind| kind == bootstrap.active_panel_kind.as_str())
                || guide.applies_to.iter().any(|kind| kind == "any")
        })
        .cloned()
        .collect::<Vec<_>>();
    let operations = crate::storage::Storage::open(paths)?
        .list_agent_operations(Some(&paths.context_id), Some("active"))?;
    payload["protocolVersion"] = json!(crate::operations::AGENT_PROTOCOL_VERSION);
    payload["activePanelId"] = json!(bootstrap.active_panel_id);
    payload["activePanelKind"] = json!(bootstrap.active_panel_kind);
    payload["focus"] = json!({
        "focusRevision": crate::control::read_focus_revision(paths)?,
        "projectId": bootstrap.session.id,
        "panelId": bootstrap.panel.id,
        "panelKind": bootstrap.panel.kind,
    });
    payload["studioBinding"] = json!({
        "contextId": paths.context_id,
        "contextIdSource": paths.context_id_source,
        "projectDir": paths.project_dir,
    });
    payload["applicableGuides"] = json!(applicable_guides);
    payload["activeOperations"] = json!(operations);
    payload["entrySkill"] = json!({
        "id": "myopenpanels",
        "requiredVersion": MYOPENPANELS_SKILL_VERSION,
        "source": MYOPENPANELS_SKILL_SOURCE,
        "updateWhen": "Update only during this startup Bootstrap when the loaded Skill version is missing or lower than requiredVersion.",
        "instruction": "Compare the loaded MyOpenPanels Skill metadata.version with requiredVersion. If it is missing or lower, use the Agent environment's Skill installer to reinstall or update MyOpenPanels from source, then continue with this Bootstrap response. Do not update when the installed version is equal or newer."
    });
    payload["nextRequiredAction"] = json!({
        "intent": "match-user-request",
        "instruction": "Match the user request against applicableGuides.loadWhen. Start complex writes with the advertised generation workflow before invoking an external model."
    });
    Ok(payload)
}

pub fn list_agent_guides() -> Result<Vec<AgentGuideMetadata>, CliError> {
    Ok(load_agent_guides()?
        .into_iter()
        .map(|guide| guide.metadata)
        .collect())
}

pub fn read_agent_guide(
    paths: &OpenPanelsPaths,
    guide_id: &str,
    task_id: Option<&str>,
) -> Result<AgentGuideReadPayload, CliError> {
    let guide = load_agent_guides()?
        .into_iter()
        .find(|guide| guide.metadata.id == guide_id)
        .ok_or_else(|| CliError::new(format!("OpenPanels agent guide not found: {guide_id}")))?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let selection = read_selection(paths, None, false).ok();
    let wiki_selection = read_agent_selection(paths).ok();
    let markdown = render_agent_guide(
        &guide,
        &bootstrap,
        selection.as_ref(),
        wiki_selection.as_ref(),
        task_id,
    )?;
    Ok(AgentGuideReadPayload {
        guide: guide.metadata,
        markdown,
    })
}

pub fn sync_builtin_agent_skills(paths: &OpenPanelsPaths) -> Result<(), CliError> {
    let skills_dir = paths.storage_dir.join("skills");
    fs::create_dir_all(&skills_dir).map_err(to_cli_error)?;
    for (skill, skill_dir) in load_agent_skill_dirs()? {
        let local_dir = skills_dir.join(&skill.metadata.id);
        if local_dir.exists() {
            fs::remove_dir_all(&local_dir).map_err(to_cli_error)?;
        }
        fs::create_dir_all(&local_dir).map_err(to_cli_error)?;
        extract_embedded_dir_contents(skill_dir, skill_dir.path(), &local_dir)?;
    }
    Ok(())
}

pub fn list_agent_skills(paths: &OpenPanelsPaths) -> Result<Vec<AgentSkillListing>, CliError> {
    sync_builtin_agent_skills(paths)?;
    Ok(load_agent_skills()?
        .into_iter()
        .map(|skill| agent_skill_listing(paths, skill.metadata))
        .collect())
}

pub fn read_agent_skill(
    paths: &OpenPanelsPaths,
    skill_id: &str,
    task_id: Option<&str>,
) -> Result<AgentSkillReadPayload, CliError> {
    sync_builtin_agent_skills(paths)?;
    let skill = load_agent_skills()?
        .into_iter()
        .find(|skill| skill.metadata.id == skill_id)
        .ok_or_else(|| CliError::new(format!("OpenPanels agent skill not found: {skill_id}")))?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let selection = read_selection(paths, None, false).ok();
    let wiki_selection = read_agent_selection(paths).ok();
    let (local_dir, local_path) = agent_skill_local_paths(paths, &skill.metadata.id);
    let markdown = render_agent_skill(
        &skill,
        &bootstrap,
        selection.as_ref(),
        wiki_selection.as_ref(),
        task_id,
        &local_dir,
        &local_path,
    )?;
    Ok(AgentSkillReadPayload {
        skill: skill.metadata,
        local_dir: local_dir.display().to_string(),
        local_path: local_path.display().to_string(),
        markdown,
    })
}

pub fn capabilities() -> Vec<Value> {
    vec![
        capability(
            "agent.bootstrap.read",
            "Read protocol v2 context, guides, and active operations",
            "openpanels-local agent bootstrap",
            "current_studio_or_storage",
            false,
            vec![],
        ),
        capability(
            "agent.guides.list",
            "List loadable agent guides",
            "openpanels-local agent guides",
            "none",
            false,
            vec![],
        ),
        capability(
            "agent.skills.list",
            "List loadable agent skills",
            "openpanels-local agent skills",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "agent.bridge.run",
            "Run the task bridge",
            "openpanels-local agent bridge",
            "current_user_project",
            false,
            vec![
                arg("command", "--command", "string", true),
                arg("capability", "--capability", "string", false),
                arg("name", "--name", "string", false),
                arg("once", "--once", "bool", false),
                arg("queue", "--queue", "string", false),
                arg("timeoutMs", "--timeout-ms", "number", false),
                arg("manualLifecycle", "--manual-lifecycle", "bool", false),
            ],
        ),
        capability(
            "agent.bridge.status",
            "Read task bridge status",
            "openpanels-local agent bridge status",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "agent.targets.list",
            "List registered agent targets",
            "openpanels-local agent targets list",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "agent.targets.register",
            "Register an agent target",
            "openpanels-local agent targets register",
            "current_user_project",
            false,
            vec![
                arg("name", "--name", "string", true),
                enum_arg(
                    "transport",
                    "--transport",
                    true,
                    &["webhook", "poll", "command"],
                    None,
                ),
                arg("capability", "--capability", "string", true),
                arg("endpoint", "--endpoint", "string", false),
                arg("priority", "--priority", "number", false),
            ],
        ),
        capability(
            "agent.targets.heartbeat",
            "Heartbeat an agent target",
            "openpanels-local agent targets heartbeat",
            "current_user_project",
            false,
            vec![arg("targetId", "--target-id", "string", true)],
        ),
        capability(
            "agent.targets.remove",
            "Remove an agent target",
            "openpanels-local agent targets remove",
            "current_user_project",
            false,
            vec![arg("targetId", "--target-id", "string", true)],
        ),
        capability(
            "agent.guide.read",
            "Read one full agent guide",
            "openpanels-local agent guide <guide-id>",
            "current_user_project",
            false,
            vec![arg("taskId", "--task-id", "string", false)],
        ),
        capability(
            "agent.skill.read",
            "Read one full agent skill",
            "openpanels-local agent skill <skill-id>",
            "current_user_project",
            false,
            vec![arg("taskId", "--task-id", "string", false)],
        ),
        capability(
            "project.current.read",
            "Read the current user-visible project",
            "openpanels-local project current",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "project.list",
            "List available projects",
            "openpanels-local project list",
            "current_studio_or_storage",
            false,
            vec![],
        ),
        capability(
            "project.create",
            "Create a project explicitly",
            "openpanels-local project create",
            "storage",
            true,
            vec![arg("title", "--title", "string", false)],
        ),
        capability(
            "panel.list",
            "List panels in the current project",
            "openpanels-local panel list",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "panel.current.read",
            "Read the current panel",
            "openpanels-local panel current",
            "current_user_project.current_panel",
            false,
            vec![],
        ),
        capability(
            "panel.switch",
            "Switch the current panel by kind",
            "openpanels-local panel switch",
            "current_user_project",
            false,
            vec![enum_arg(
                "kind",
                "--kind",
                true,
                &["wiki", "canvas", "image", "diff", "preview", "files"],
                None,
            )],
        ),
        capability(
            "canvas.state.read",
            "Read current canvas state",
            "openpanels-local canvas state",
            "current_user_project.current_canvas",
            false,
            vec![],
        ),
        capability(
            "canvas.selection.read",
            "Read current canvas selection",
            "openpanels-local canvas selection read",
            "current_user_project.current_canvas",
            false,
            vec![],
        ),
        capability(
            "canvas.selection.asset.read",
            "Export selected canvas pixels to a file",
            "openpanels-local canvas selection export",
            "current_user_project.current_canvas",
            false,
            vec![
                arg("output", "--output", "path", true),
                arg("allowFallback", "--allow-fallback", "bool", false),
            ],
        ),
        capability(
            "canvas.generation.begin",
            "Start a target-bound Canvas image generation and create its placeholder",
            "openpanels-local canvas generation begin",
            "current_user_project.current_canvas",
            false,
            vec![
                arg("displayWidth", "--display-width", "number", false),
                arg("displayHeight", "--display-height", "number", false),
                arg("useSelection", "--use-selection", "bool", false),
                arg("text", "--text", "string", false),
            ],
        ),
        capability(
            "canvas.generation.complete",
            "Complete a Canvas generation against its captured target",
            "openpanels-local canvas generation complete",
            "agent_operation.target_canvas",
            false,
            vec![
                arg("operationId", "--operation-id", "string", true),
                arg("image", "--image", "path", true),
                arg("metadataFile", "--metadata-file", "path", true),
            ],
        ),
        capability(
            "wiki.generation.begin",
            "Start a target-bound Wiki document generation",
            "openpanels-local wiki generation begin",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("title", "--title", "string", true),
                arg("documentFormat", "--document-format", "enum", false),
                arg("documentId", "--document-id", "string", false),
            ],
        ),
        capability(
            "wiki.generation.complete",
            "Complete a Wiki generation against its captured target",
            "openpanels-local wiki generation complete",
            "agent_operation.target_wiki",
            false,
            vec![
                arg("operationId", "--operation-id", "string", true),
                arg("file", "--file", "path", true),
            ],
        ),
        capability(
            "canvas.image.insert",
            "Insert a local image into the current canvas",
            "openpanels-local canvas image insert",
            "current_user_project.current_canvas",
            false,
            vec![
                arg("image", "--image", "path", true),
                enum_arg(
                    "placement",
                    "--placement",
                    false,
                    &["auto", "right", "below", "left"],
                    Some("auto"),
                ),
                arg("metadataFile", "--metadata-file", "path", false),
                arg("replaceShapeId", "--replace-shape-id", "string", false),
            ],
        ),
        capability(
            "tasks.list",
            "List project tasks across panels",
            "openpanels-local tasks list",
            "current_user_project",
            false,
            vec![
                arg("pending", "--pending", "bool", false),
                arg("queue", "--queue", "string", false),
                arg("status", "--status", "string", false),
            ],
        ),
        capability(
            "tasks.next",
            "Read the next pending project task",
            "openpanels-local tasks next",
            "current_user_project",
            false,
            vec![
                arg("queue", "--queue", "string", false),
                arg("status", "--status", "string", false),
            ],
        ),
        capability(
            "tasks.inspect",
            "Read one project task by id",
            "openpanels-local tasks inspect",
            "current_user_project",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "tasks.claimNext",
            "Atomically claim the next matching task",
            "openpanels-local tasks claim-next",
            "current_user_project",
            false,
            vec![
                arg("targetId", "--target-id", "string", true),
                arg("capability", "--capability", "string", false),
                arg("waitMs", "--wait-ms", "number", false),
            ],
        ),
        capability(
            "tasks.claim",
            "Atomically claim one task",
            "openpanels-local tasks claim",
            "current_user_project",
            false,
            vec![
                arg("taskId", "--task-id", "string", true),
                arg("targetId", "--target-id", "string", true),
            ],
        ),
        capability(
            "tasks.heartbeat",
            "Extend a task lease",
            "openpanels-local tasks heartbeat",
            "current_user_project",
            false,
            vec![
                arg("taskId", "--task-id", "string", true),
                arg("leaseToken", "--lease-token", "string", true),
            ],
        ),
        capability(
            "tasks.complete",
            "Complete a claimed task",
            "openpanels-local tasks complete",
            "current_user_project",
            false,
            vec![
                arg("taskId", "--task-id", "string", true),
                arg("leaseToken", "--lease-token", "string", true),
                arg("resultFile", "--result-file", "path", false),
            ],
        ),
        capability(
            "tasks.fail",
            "Fail a claimed task",
            "openpanels-local tasks fail",
            "current_user_project",
            false,
            vec![
                arg("taskId", "--task-id", "string", true),
                arg("leaseToken", "--lease-token", "string", true),
                arg("message", "--message", "string", true),
                arg("retryAfter", "--retry-after", "string", false),
            ],
        ),
        capability(
            "tasks.release",
            "Release a claimed task",
            "openpanels-local tasks release",
            "current_user_project",
            false,
            vec![
                arg("taskId", "--task-id", "string", true),
                arg("leaseToken", "--lease-token", "string", true),
            ],
        ),
        capability(
            "tasks.retry",
            "Manually retry a failed task",
            "openpanels-local tasks retry",
            "current_user_project",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "tasks.cancel",
            "Cancel a task",
            "openpanels-local tasks cancel",
            "current_user_project",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "wiki.context.read",
            "Read current wiki context",
            "openpanels-local wiki context",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.selection.read",
            "Read the user-selected Wiki and raw documents",
            "openpanels-local wiki selection read",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.document.list",
            "List raw wiki documents",
            "openpanels-local wiki documents list",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.document.add",
            "Add a raw wiki document",
            "openpanels-local wiki documents add",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("file", "--file", "path", true),
                arg("title", "--title", "string", false),
            ],
        ),
        capability(
            "wiki.document.createMarkdown",
            "Create a markdown raw document",
            "openpanels-local wiki documents create-markdown",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("title", "--title", "string", true),
                arg("file", "--file", "path", false),
                arg("content", "--content", "string", false),
            ],
        ),
        capability(
            "wiki.markdown.read",
            "Read markdown for a raw document",
            "openpanels-local wiki markdown read",
            "current_user_project.current_wiki",
            false,
            vec![arg("documentId", "--document-id", "string", true)],
        ),
        capability(
            "wiki.markdown.write",
            "Write markdown for a raw document",
            "openpanels-local wiki markdown write",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("documentId", "--document-id", "string", true),
                arg("file", "--file", "path", true),
                arg("taskId", "--task-id", "string", false),
            ],
        ),
        capability(
            "wiki.generatedDocument.list",
            "List agent-generated documents",
            "openpanels-local wiki generated-documents list",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.generatedDocument.read",
            "Read an agent-generated document",
            "openpanels-local wiki generated-documents read",
            "current_user_project.current_wiki",
            false,
            vec![arg("documentId", "--document-id", "string", true)],
        ),
        capability(
            "wiki.generatedDocument.rename",
            "Rename an agent-generated document",
            "openpanels-local wiki generated-documents rename",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("documentId", "--document-id", "string", true),
                arg("title", "--title", "string", true),
            ],
        ),
        capability(
            "wiki.generatedDocument.delete",
            "Delete an agent-generated document",
            "openpanels-local wiki generated-documents delete",
            "current_user_project.current_wiki",
            false,
            vec![arg("documentId", "--document-id", "string", true)],
        ),
        capability(
            "wiki.generatedDocument.publish",
            "Publish an agent-generated document as a raw Wiki document",
            "openpanels-local wiki generated-documents publish",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("documentId", "--document-id", "string", true),
                arg("wikiSpaceId", "--wiki-space-id", "string", false),
            ],
        ),
        capability(
            "wiki.task.list",
            "List wiki tasks",
            "openpanels-local wiki tasks list",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.task.next",
            "Read the next wiki task",
            "openpanels-local wiki tasks next",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.task.claim",
            "Claim a wiki task",
            "openpanels-local wiki tasks claim",
            "current_user_project.current_wiki",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "wiki.task.complete",
            "Complete a wiki task",
            "openpanels-local wiki tasks complete",
            "current_user_project.current_wiki",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "wiki.task.fail",
            "Fail a wiki task",
            "openpanels-local wiki tasks fail",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("taskId", "--task-id", "string", true),
                arg("message", "--message", "string", true),
            ],
        ),
        capability(
            "wiki.space.list",
            "List wiki spaces",
            "openpanels-local wiki spaces list",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.space.switch",
            "Switch active wiki space",
            "openpanels-local wiki spaces switch",
            "current_user_project.current_wiki",
            false,
            vec![arg("wikiSpaceId", "--wiki-space-id", "string", true)],
        ),
        capability(
            "wiki.page.list",
            "List pages in a wiki space",
            "openpanels-local wiki pages list",
            "current_user_project.current_wiki",
            false,
            vec![arg("wikiSpaceId", "--wiki-space-id", "string", true)],
        ),
        capability(
            "wiki.page.read",
            "Read one wiki page",
            "openpanels-local wiki pages read",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("wikiSpaceId", "--wiki-space-id", "string", true),
                arg("path", "--path", "path", true),
            ],
        ),
        capability(
            "wiki.page.search",
            "Search generated Wiki pages",
            "openpanels-local wiki pages search",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("wikiSpaceId", "--wiki-space-id", "string", true),
                arg("query", "--query", "string", true),
                arg("limit", "--limit", "number", false),
            ],
        ),
        capability(
            "wiki.page.write",
            "Write one wiki page",
            "openpanels-local wiki pages write",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("wikiSpaceId", "--wiki-space-id", "string", true),
                arg("path", "--path", "path", true),
                arg("file", "--file", "path", true),
                arg("taskId", "--task-id", "string", false),
            ],
        ),
        capability(
            "studio.start",
            "Start or reuse the local Studio",
            "openpanels-local studio start",
            "project_directory",
            false,
            vec![],
        ),
        capability(
            "studio.status",
            "Read local Studio process status",
            "openpanels-local studio status",
            "project_directory",
            false,
            vec![],
        ),
        capability(
            "studio.stop",
            "Stop the local Studio binding",
            "openpanels-local studio stop",
            "project_directory",
            false,
            vec![],
        ),
    ]
}

fn capability(
    intent: &str,
    title: &str,
    command: &str,
    target: &str,
    creates_project: bool,
    args: Vec<Value>,
) -> Value {
    let related_guides = if intent.starts_with("canvas.generation") {
        json!(["canvas.image-generation"])
    } else if intent.starts_with("wiki.generation") {
        json!(["wiki.generated-documents"])
    } else {
        json!([])
    };
    let preconditions = if intent.ends_with("generation.complete") {
        json!(["active_operation"])
    } else if intent.ends_with("generation.begin") {
        json!(["fresh_agent_bootstrap", "matching_active_panel"])
    } else {
        json!([])
    };
    json!({
        "intent": intent,
        "title": title,
        "command": command,
        "target": target,
        "createsProject": creates_project,
        "args": args,
        "output": "json",
        "relatedGuides": related_guides,
        "preconditions": preconditions,
        "workflowIntent": if intent.contains("generation") { Some(intent) } else { None },
    })
}

fn arg(name: &str, flag: &str, value_type: &str, required: bool) -> Value {
    json!({
        "name": name,
        "flag": flag,
        "type": value_type,
        "required": required,
    })
}

fn enum_arg(
    name: &str,
    flag: &str,
    required: bool,
    values: &[&str],
    default: Option<&str>,
) -> Value {
    let mut value = arg(name, flag, "enum", required);
    if let Some(object) = value.as_object_mut() {
        object.insert("values".to_owned(), json!(values));
        if let Some(default) = default {
            object.insert("default".to_owned(), json!(default));
        }
    }
    value
}

pub fn render_agent_guides_markdown(guides: &[AgentGuideMetadata]) -> String {
    format!(
        "# OpenPanels Agent Guides\n\n{}\n",
        render_guide_table(guides)
    )
}

pub fn render_agent_skills_markdown(skills: &[AgentSkillListing]) -> String {
    format!(
        "# OpenPanels Agent Skills\n\n{}\n",
        render_skill_table(skills)
    )
}

fn agent_bootstrap_payload(
    bootstrap: &ProjectBootstrap,
    cli_version: &str,
    guides: &[AgentGuideMetadata],
    skills: &[AgentSkillListing],
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
) -> Value {
    let wiki = wiki_summary(bootstrap, wiki_selection);
    let knowledge = knowledge_context(&wiki);
    let tasks = project_tasks_summary(bootstrap);
    let canvas = canvas_summary(selection);
    json!({
        "protocolVersion": crate::operations::AGENT_PROTOCOL_VERSION,
        "cliVersion": cli_version,
        "project": {
            "id": bootstrap.session.id,
            "title": bootstrap.session.title,
        },
        "activePanel": {
            "id": bootstrap.active_panel_id,
            "kind": bootstrap.active_panel_kind,
            "title": bootstrap.panel.title,
        },
        "panels": bootstrap.panels.iter().map(|snapshot| json!({
            "id": snapshot.panel.id,
            "kind": snapshot.panel.kind,
            "title": snapshot.panel.title,
        })).collect::<Vec<_>>(),
        "state": {
            "tasks": tasks,
            "wiki": wiki,
            "canvas": canvas,
        },
        "knowledgeContext": knowledge,
        "capabilities": capabilities(),
        "suggestedCommands": suggested_commands(bootstrap, guides, skills, wiki_selection),
        "availableGuides": guides,
        "availableSkills": skills,
    })
}

fn render_agent_guide(
    guide: &AgentGuide,
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
    task_id: Option<&str>,
) -> Result<String, CliError> {
    let task = task_id.and_then(|id| find_wiki_task(bootstrap, id));
    if task_id.is_some() && task.is_none() {
        return Err(CliError::new(format!(
            "Wiki task not found: {}",
            task_id.unwrap_or_default()
        )));
    }
    Ok(format!(
        "# Guide: {}\n\nTitle: {}\nSource: {}\nApplies to: {}\n\n## Current Context\n\n{}\n\n## Commands For This Guide\n\n{}\n\n## Instructions\n\n{}\n",
        guide.metadata.id,
        guide.metadata.title,
        guide.metadata.source,
        if guide.metadata.applies_to.is_empty() { "any".to_owned() } else { guide.metadata.applies_to.join(", ") },
        render_current_context(bootstrap, selection, wiki_selection, task.as_ref()),
        render_guide_commands(guide, task.as_ref()),
        guide.body.trim(),
    ))
}

fn render_agent_skill(
    skill: &AgentSkill,
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
    task_id: Option<&str>,
    local_dir: &Path,
    local_path: &Path,
) -> Result<String, CliError> {
    let task = task_id.and_then(|id| find_wiki_task(bootstrap, id));
    if task_id.is_some() && task.is_none() {
        return Err(CliError::new(format!(
            "Wiki task not found: {}",
            task_id.unwrap_or_default()
        )));
    }
    Ok(format!(
        "# Skill: {}\n\nTitle: {}\nSource: {}\nLocal dir: {}\nLocal path: {}\nApplies to: {}\n\n## How To Load This Skill\n\nRead `SKILL.md` directly from the local path above. Treat this CLI output as the task-specific loader and command context, not as the skill body. Resolve referenced files relative to the local dir above.\n\n## Current Context\n\n{}\n\n## Commands For This Skill\n\n{}\n",
        skill.metadata.id,
        skill.metadata.title,
        skill.metadata.source,
        local_dir.display(),
        local_path.display(),
        if skill.metadata.applies_to.is_empty() { "any".to_owned() } else { skill.metadata.applies_to.join(", ") },
        render_current_context(bootstrap, selection, wiki_selection, task.as_ref()),
        render_skill_commands(skill, task.as_ref(), wiki_selection),
    ))
}

fn load_agent_guides() -> Result<Vec<AgentGuide>, CliError> {
    if let Ok(dir) = std::env::var("OPENPANELS_AGENT_GUIDES_DIR") {
        if !dir.trim().is_empty() {
            return load_agent_guides_from_dir(PathBuf::from(dir));
        }
    }
    let mut guides = AGENT_GUIDES
        .files()
        .filter(|file| file.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        .map(|file| {
            let name = file.path().display().to_string();
            let source = std::str::from_utf8(file.contents()).map_err(to_cli_error)?;
            parse_guide(source, &name)
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    guides.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(guides)
}

fn load_agent_guides_from_dir(dir: PathBuf) -> Result<Vec<AgentGuide>, CliError> {
    let mut guides = Vec::new();
    for entry in fs::read_dir(dir).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(to_cli_error)?;
        guides.push(parse_guide(&source, &path.display().to_string())?);
    }
    guides.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(guides)
}

fn load_agent_skills() -> Result<Vec<AgentSkill>, CliError> {
    let mut skills = load_agent_skill_dirs()?
        .into_iter()
        .map(|(skill, _dir)| skill)
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(skills)
}

fn load_agent_skill_dirs() -> Result<Vec<(AgentSkill, &'static Dir<'static>)>, CliError> {
    let mut seen = BTreeSet::new();
    let mut skills = Vec::new();
    for dir in AGENT_SKILLS.dirs() {
        let skill_path = dir.path().join("SKILL.md");
        let file = AGENT_SKILLS.get_file(&skill_path).ok_or_else(|| {
            CliError::new(format!(
                "OpenPanels agent skill is missing SKILL.md: {}",
                dir.path().display()
            ))
        })?;
        let source = std::str::from_utf8(file.contents()).map_err(to_cli_error)?;
        let skill = parse_skill(source, &skill_path.display().to_string())?;
        if !seen.insert(skill.metadata.id.clone()) {
            return Err(CliError::new(format!(
                "Duplicate OpenPanels agent skill id: {}",
                skill.metadata.id
            )));
        }
        skills.push((skill, dir));
    }
    skills.sort_by(|left, right| left.0.metadata.id.cmp(&right.0.metadata.id));
    Ok(skills)
}

fn extract_embedded_dir_contents(
    dir: &Dir<'_>,
    root: &Path,
    destination: &Path,
) -> Result<(), CliError> {
    for file in dir.files() {
        let relative_path = file.path().strip_prefix(root).map_err(to_cli_error)?;
        let target_path = destination.join(relative_path);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(target_path, file.contents()).map_err(to_cli_error)?;
    }
    for child_dir in dir.dirs() {
        extract_embedded_dir_contents(child_dir, root, destination)?;
    }
    Ok(())
}

fn parse_guide(source: &str, file_name: &str) -> Result<AgentGuide, CliError> {
    let normalized_source;
    let source = if source.contains("\r\n") {
        normalized_source = source.replace("\r\n", "\n");
        normalized_source.as_str()
    } else {
        source
    };
    let rest = source
        .strip_prefix("---\n")
        .ok_or_else(|| CliError::new(format!("Agent guide is missing frontmatter: {file_name}")))?;
    let (frontmatter, body) = rest
        .split_once("\n---")
        .ok_or_else(|| CliError::new(format!("Agent guide is missing frontmatter: {file_name}")))?;
    let frontmatter = parse_frontmatter(frontmatter);
    let id = scalar(&frontmatter, "id")
        .ok_or_else(|| CliError::new(format!("Agent guide requires id and title: {file_name}")))?;
    let title = scalar(&frontmatter, "title")
        .ok_or_else(|| CliError::new(format!("Agent guide requires id and title: {file_name}")))?;
    Ok(AgentGuide {
        metadata: AgentGuideMetadata {
            applies_to: list(&frontmatter, "appliesTo"),
            id,
            load_when: list(&frontmatter, "loadWhen"),
            requires_capabilities: list(&frontmatter, "requiresCapabilities"),
            source: scalar(&frontmatter, "source").unwrap_or_else(|| "builtin".to_owned()),
            task_types: list(&frontmatter, "taskTypes"),
            title,
            tokens: scalar(&frontmatter, "tokens").unwrap_or_else(|| "medium".to_owned()),
        },
        body: body.trim_start_matches('\n').to_owned(),
    })
}

fn parse_skill(source: &str, file_name: &str) -> Result<AgentSkill, CliError> {
    let normalized_source;
    let source = if source.contains("\r\n") {
        normalized_source = source.replace("\r\n", "\n");
        normalized_source.as_str()
    } else {
        source
    };
    let rest = source
        .strip_prefix("---\n")
        .ok_or_else(|| CliError::new(format!("Agent skill is missing frontmatter: {file_name}")))?;
    let (frontmatter, body) = rest
        .split_once("\n---")
        .ok_or_else(|| CliError::new(format!("Agent skill is missing frontmatter: {file_name}")))?;
    let frontmatter = parse_frontmatter(frontmatter);
    let id = scalar(&frontmatter, "id")
        .ok_or_else(|| CliError::new(format!("Agent skill requires id and title: {file_name}")))?;
    let title = scalar(&frontmatter, "title")
        .ok_or_else(|| CliError::new(format!("Agent skill requires id and title: {file_name}")))?;
    Ok(AgentSkill {
        metadata: AgentSkillMetadata {
            applies_to: list(&frontmatter, "appliesTo"),
            description: scalar(&frontmatter, "description").unwrap_or_default(),
            id,
            load_when: list(&frontmatter, "loadWhen"),
            requires_capabilities: list(&frontmatter, "requiresCapabilities"),
            source: scalar(&frontmatter, "source").unwrap_or_else(|| "builtin".to_owned()),
            task_types: list(&frontmatter, "taskTypes"),
            title,
            tokens: scalar(&frontmatter, "tokens").unwrap_or_else(|| "medium".to_owned()),
        },
        body: body.trim_start_matches('\n').to_owned(),
    })
}

fn parse_frontmatter(source: &str) -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::new();
    let mut current_key: Option<String> = None;
    for line in source.lines() {
        if let Some(value) = line.trim_start().strip_prefix("- ") {
            if let Some(key) = &current_key {
                result
                    .entry(key.clone())
                    .or_insert_with(Vec::new)
                    .push(value.trim().to_owned());
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            current_key = Some(key.to_owned());
            result.insert(
                key.to_owned(),
                if value.is_empty() {
                    Vec::new()
                } else {
                    vec![value.to_owned()]
                },
            );
        }
    }
    result
}

fn scalar(frontmatter: &BTreeMap<String, Vec<String>>, key: &str) -> Option<String> {
    frontmatter
        .get(key)
        .and_then(|values| values.first())
        .cloned()
}

fn list(frontmatter: &BTreeMap<String, Vec<String>>, key: &str) -> Vec<String> {
    frontmatter.get(key).cloned().unwrap_or_default()
}

fn render_guide_table(guides: &[AgentGuideMetadata]) -> String {
    if guides.is_empty() {
        return "- none".to_owned();
    }
    let rows = guides
        .iter()
        .map(|guide| {
            format!(
                "| `{}` | {} | {} | {} | {} |",
                guide.id,
                guide.source,
                guide.applies_to.join(", "),
                guide.task_types.join(", "),
                guide.load_when.join("; ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("| ID | Source | Applies To | Task Types | Load When |\n| --- | --- | --- | --- | --- |\n{rows}")
}

fn render_skill_table(skills: &[AgentSkillListing]) -> String {
    if skills.is_empty() {
        return "- none".to_owned();
    }
    let rows = skills
        .iter()
        .map(|item| {
            format!(
                "| `{}` | {} | {} | {} | {} |",
                item.skill.id,
                item.source,
                item.skill.applies_to.join(", "),
                item.skill.task_types.join(", "),
                item.local_path
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("| ID | Source | Applies To | Task Types | Local Path |\n| --- | --- | --- | --- | --- |\n{rows}")
}

fn agent_skill_listing(paths: &OpenPanelsPaths, skill: AgentSkillMetadata) -> AgentSkillListing {
    let (local_dir, local_path) = agent_skill_local_paths(paths, &skill.id);
    AgentSkillListing {
        source: skill.source.clone(),
        skill,
        local_dir: local_dir.display().to_string(),
        local_path: local_path.display().to_string(),
    }
}

fn agent_skill_local_paths(paths: &OpenPanelsPaths, skill_id: &str) -> (PathBuf, PathBuf) {
    let local_dir = paths.storage_dir.join("skills").join(skill_id);
    let local_path = local_dir.join("SKILL.md");
    (local_dir, local_path)
}

fn wiki_summary(bootstrap: &ProjectBootstrap, selection: Option<&Value>) -> Value {
    let state = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .map(|snapshot| &snapshot.state)
        .unwrap_or(&bootstrap.state);
    let tasks = state
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|task| {
            task.get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| ["queued", "claimed", "running", "failed"].contains(&status))
        })
        .collect::<Vec<_>>();
    let next_task = tasks
        .iter()
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            tasks
                .iter()
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
        .or_else(|| tasks.first())
        .cloned();
    let active_space_id = state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let active_space = state
        .get("wikiSpaces")
        .and_then(Value::as_array)
        .and_then(|spaces| {
            spaces
                .iter()
                .find(|space| space.get("id").and_then(Value::as_str) == Some(active_space_id))
                .or_else(|| spaces.first())
        });
    let selected_documents = selection
        .and_then(|value| value.get("selectedRawDocuments"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|document| {
            json!({
                "documentId": document.get("id").cloned().unwrap_or(Value::Null),
                "title": document.get("title").cloned().unwrap_or(Value::Null),
                "mimeType": document.get("mimeType").cloned().unwrap_or(Value::Null),
                "markdownVersion": document.get("markdownVersion").cloned().unwrap_or(Value::Null),
                "originalFilePath": document.get("originalFilePath").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    let selected_generated_documents = selection
        .and_then(|value| value.get("selectedGeneratedDocuments"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|document| {
            json!({
                "documentId": document.get("id").cloned().unwrap_or(Value::Null),
                "title": document.get("title").cloned().unwrap_or(Value::Null),
                "format": document.get("format").cloned().unwrap_or(Value::Null),
                "contentVersion": document.get("contentVersion").cloned().unwrap_or(Value::Null),
                "contentFilePath": document.get("contentFilePath").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "agentSkillId": selected_agent_skill_id(state),
        "available": state.get("wikiSpaces").and_then(Value::as_array).is_some_and(|spaces| !spaces.is_empty()),
        "selected": selection.and_then(|value| value.get("selection")).and_then(|value| value.get("isWikiSelected")).and_then(Value::as_bool).unwrap_or(false),
        "wikiSpaceId": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("wikiSpaceId")).cloned().unwrap_or_else(|| json!(active_space_id)),
        "wikiTitle": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("title")).cloned().or_else(|| active_space.and_then(|space| space.get("title")).cloned()).unwrap_or_else(|| json!("Wiki")),
        "pageCount": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("pageCount")).cloned().unwrap_or_else(|| json!(active_space.and_then(|space| space.get("pageIndex")).and_then(Value::as_array).map(Vec::len).unwrap_or(0))),
        "querySkillId": WIKI_QUERY_SKILL_ID,
        "querySkillLoadCommand": format!("openpanels-local agent skill {WIKI_QUERY_SKILL_ID} --format json"),
        "selectedRawDocumentCount": selected_documents.len(),
        "selectedRawDocuments": selected_documents,
        "selectedGeneratedDocumentCount": selected_generated_documents.len(),
        "selectedGeneratedDocuments": selected_generated_documents,
        "nextTask": next_task,
        "pendingTaskCount": tasks.len(),
    })
}

fn knowledge_context(wiki: &Value) -> Value {
    json!({
        "guideId": WIKI_KNOWLEDGE_GUIDE_ID,
        "guideLoadCommand": format!("openpanels-local agent guide {WIKI_KNOWLEDGE_GUIDE_ID} --format json"),
        "wiki": {
            "available": wiki.get("available").cloned().unwrap_or_else(|| json!(false)),
            "selected": wiki.get("selected").cloned().unwrap_or_else(|| json!(false)),
            "wikiSpaceId": wiki.get("wikiSpaceId").cloned().unwrap_or(Value::Null),
            "title": wiki.get("wikiTitle").cloned().unwrap_or(Value::Null),
            "pageCount": wiki.get("pageCount").cloned().unwrap_or_else(|| json!(0)),
            "querySkillId": wiki.get("querySkillId").cloned().unwrap_or_else(|| json!(WIKI_QUERY_SKILL_ID)),
            "loadCommand": wiki.get("querySkillLoadCommand").cloned().unwrap_or(Value::Null),
        },
        "rawDocuments": {
            "selected": wiki.get("selectedRawDocuments").cloned().unwrap_or_else(|| json!([])),
        },
        "generatedDocuments": {
            "guideId": WIKI_GENERATED_DOCUMENTS_GUIDE_ID,
            "guideLoadCommand": format!("openpanels-local agent guide {WIKI_GENERATED_DOCUMENTS_GUIDE_ID} --format json"),
            "selected": wiki.get("selectedGeneratedDocuments").cloned().unwrap_or_else(|| json!([])),
        },
    })
}

fn project_tasks_summary(bootstrap: &ProjectBootstrap) -> Value {
    let next_task = next_project_task(bootstrap).cloned();
    json!({
        "nextTask": next_task,
        "pendingTaskCount": bootstrap.pending_task_count,
        "totalTaskCount": bootstrap.tasks.len(),
    })
}

fn canvas_summary(selection: Option<&crate::selection::SelectionPayload>) -> Value {
    let is_explicit_selection = selection
        .and_then(|selection| selection.selection.get("isExplicitSelection"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let selected_shapes = selection
        .and_then(|selection| selection.selection.get("selectedShapes"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let selected_ids = selection
        .and_then(|selection| selection.selection.get("selectedShapeIds"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    json!({
        "fallback": selection.and_then(|selection| selection.selection.get("fallback")).and_then(Value::as_str),
        "hasSelectedImageAsset": is_explicit_selection && selection.and_then(|selection| selection.selection.get("assetRef")).and_then(Value::as_str).is_some(),
        "hasSelection": is_explicit_selection && (selected_shapes > 0 || selected_ids > 0),
        "isExplicitSelection": is_explicit_selection,
        "selectedShapeCount": if is_explicit_selection { if selected_shapes > 0 { selected_shapes } else { selected_ids } } else { 0 },
    })
}

fn suggested_commands(
    bootstrap: &ProjectBootstrap,
    guides: &[AgentGuideMetadata],
    skills: &[AgentSkillListing],
    wiki_selection: Option<&Value>,
) -> Vec<Value> {
    let mut commands = Vec::new();
    if let Some(task) = next_project_task(bootstrap) {
        commands.push(json!({
            "intent": "tasks.next",
            "command": "openpanels-local tasks next --format json",
        }));
        if let Some(task_id) = task.get("id").and_then(Value::as_str) {
            commands.push(json!({
                "intent": "tasks.inspect",
                "command": format!("openpanels-local tasks inspect --task-id {} --format json", shell_quote(task_id)),
            }));
        }
    }
    let wiki = wiki_summary(bootstrap, wiki_selection);
    let has_selected_knowledge = wiki
        .get("selected")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || wiki
            .get("selectedRawDocumentCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        || wiki
            .get("selectedGeneratedDocumentCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0;
    if (bootstrap.active_panel_kind == PanelKind::Wiki || has_selected_knowledge)
        && guides
            .iter()
            .any(|guide| guide.id == WIKI_KNOWLEDGE_GUIDE_ID)
    {
        commands.push(json!({
            "intent": "agent.guide.read",
            "command": format!("openpanels-local agent guide {WIKI_KNOWLEDGE_GUIDE_ID} --format json"),
        }));
    }
    let has_wiki_panel = bootstrap
        .panels
        .iter()
        .any(|snapshot| snapshot.panel.kind == PanelKind::Wiki);
    if has_wiki_panel
        && guides
            .iter()
            .any(|guide| guide.id == WIKI_GENERATED_DOCUMENTS_GUIDE_ID)
    {
        commands.push(json!({
            "intent": "agent.guide.read",
            "command": format!("openpanels-local agent guide {WIKI_GENERATED_DOCUMENTS_GUIDE_ID} --format json"),
        }));
    }
    if let Some(task) = wiki.get("nextTask").filter(|value| !value.is_null()) {
        commands.push(json!({
            "intent": "wiki.task.next",
            "command": "openpanels-local wiki tasks next --format json",
        }));
        if let Some(task_type) = task.get("type").and_then(Value::as_str) {
            let selected_skill_id = wiki
                .get("agentSkillId")
                .and_then(Value::as_str)
                .unwrap_or("karpathy-llm-wiki");
            let selected_skill = skills
                .iter()
                .find(|skill| {
                    skill.skill.id == selected_skill_id
                        && skill
                            .skill
                            .task_types
                            .iter()
                            .any(|candidate| candidate == task_type)
                })
                .or_else(|| {
                    skills.iter().find(|skill| {
                        skill
                            .skill
                            .task_types
                            .iter()
                            .any(|candidate| candidate == task_type)
                    })
                });
            if let Some(skill) = selected_skill {
                if let Some(task_id) = task.get("id").and_then(Value::as_str) {
                    commands.push(json!({
                        "intent": "agent.skill.read",
                        "command": format!("openpanels-local agent skill {} --task-id {} --format json", skill.skill.id, task_id),
                    }));
                }
            } else if let Some(guide) = guides.iter().find(|guide| {
                guide
                    .task_types
                    .iter()
                    .any(|candidate| candidate == task_type)
            }) {
                if let Some(task_id) = task.get("id").and_then(Value::as_str) {
                    commands.push(json!({
                        "intent": "agent.guide.read",
                        "command": format!("openpanels-local agent guide {} --task-id {} --format json", guide.id, task_id),
                    }));
                }
            }
        }
    } else if bootstrap.active_panel_kind == PanelKind::Canvas {
        commands.push(json!({
            "intent": "canvas.selection.read",
            "command": "openpanels-local canvas selection read --format json",
        }));
    }
    commands
}

fn next_project_task(bootstrap: &ProjectBootstrap) -> Option<&Value> {
    bootstrap
        .tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            bootstrap
                .tasks
                .iter()
                .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':'))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn render_current_context(
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
    task: Option<&Value>,
) -> String {
    let wiki = wiki_summary(bootstrap, wiki_selection);
    let selected_shape_count = canvas_summary(selection)["selectedShapeCount"]
        .as_u64()
        .unwrap_or(0);
    let mut lines = vec![
        format!(
            "- project: {} ({})",
            bootstrap.session.title, bootstrap.session.id
        ),
        format!(
            "- active panel: {} ({})",
            bootstrap.active_panel_kind.as_str(),
            bootstrap.panel.title
        ),
        format!(
            "- wiki agent skill: {}",
            wiki["agentSkillId"].as_str().unwrap_or("karpathy-llm-wiki")
        ),
        format!(
            "- wiki selected as context: {}",
            wiki["selected"].as_bool().unwrap_or(false)
        ),
        format!(
            "- selected raw document count: {}",
            wiki["selectedRawDocumentCount"].as_u64().unwrap_or(0)
        ),
        format!("- canvas selected shape count: {selected_shape_count}"),
    ];
    if let Some(task) = task {
        lines.push(format!("- task id: {}", task["id"].as_str().unwrap_or("")));
        lines.push(format!(
            "- task type: {}",
            task["type"].as_str().unwrap_or("")
        ));
        lines.push(format!(
            "- task status: {}",
            task["status"].as_str().unwrap_or("")
        ));
        lines.push(format!(
            "- document id: {}",
            task["documentId"].as_str().unwrap_or("none")
        ));
        lines.push(format!(
            "- wiki space id: {}",
            task["wikiSpaceId"].as_str().unwrap_or("none")
        ));
    }
    lines.join("\n")
}

fn render_guide_commands(guide: &AgentGuide, task: Option<&Value>) -> String {
    if guide.metadata.id == WIKI_KNOWLEDGE_GUIDE_ID {
        return format!(
            "```bash\nopenpanels-local wiki selection read --format json\nopenpanels-local agent skill {WIKI_QUERY_SKILL_ID} --format json\n```"
        );
    }
    if let Some(task) = task {
        let task_id = task["id"].as_str().unwrap_or("<task-id>");
        let document_id = task["documentId"].as_str().unwrap_or("<document-id>");
        let wiki_space_id = task["wikiSpaceId"].as_str().unwrap_or("<wiki-space-id>");
        return format!(
            "```bash\nopenpanels-local tasks claim --task-id {task_id} --target-id <target-id> --format json\nopenpanels-local wiki markdown read --document-id {document_id} --format json\nopenpanels-local wiki pages write --wiki-space-id {wiki_space_id} --path <page-path> --file <md-file> --task-id {task_id} --format json\nopenpanels-local tasks complete --task-id {task_id} --lease-token \"$OPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
        );
    }
    if guide
        .metadata
        .applies_to
        .iter()
        .any(|value| value == "canvas")
    {
        return "```bash\nopenpanels-local canvas generation begin --display-width <w> --display-height <h> [--use-selection] --format json\nopenpanels-local canvas generation complete --operation-id <operation-id> --image <generated-path> --metadata-file <metadata.json> --format json\n```".to_owned();
    }
    "- No task-specific commands.".to_owned()
}

fn render_skill_commands(
    skill: &AgentSkill,
    task: Option<&Value>,
    wiki_selection: Option<&Value>,
) -> String {
    if skill.metadata.id == WIKI_QUERY_SKILL_ID {
        let wiki_space_id = wiki_selection
            .and_then(|selection| selection.get("wiki"))
            .and_then(|wiki| wiki.get("wikiSpaceId"))
            .and_then(Value::as_str)
            .unwrap_or("<wiki-space-id>");
        return format!(
            "```bash\nopenpanels-local wiki selection read --format json\nopenpanels-local wiki pages read --wiki-space-id {wiki_space_id} --path SCHEMA.md --format json\nopenpanels-local wiki pages read --wiki-space-id {wiki_space_id} --path index.md --format json\nopenpanels-local wiki pages search --wiki-space-id {wiki_space_id} --query <query> --format json\nopenpanels-local wiki pages read --wiki-space-id {wiki_space_id} --path <relevant-page> --format json\n```"
        );
    }
    if let Some(task) = task {
        let task_id = task["id"].as_str().unwrap_or("<task-id>");
        let document_id = task["documentId"].as_str().unwrap_or("<document-id>");
        let wiki_space_id = task["wikiSpaceId"].as_str().unwrap_or("<wiki-space-id>");
        let task_type = task["type"].as_str().unwrap_or("");
        if task_type == "convert_document_to_markdown" {
            return format!(
                "```bash\nopenpanels-local tasks claim --task-id {task_id} --target-id <target-id> --format json\nopenpanels-local wiki documents list --format json\nopenpanels-local wiki markdown write --document-id {document_id} --file <md-file> --task-id {task_id} --format json\nopenpanels-local tasks complete --task-id {task_id} --lease-token \"$OPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
            );
        }
        if task_type == "ingest_markdown_into_wiki" {
            return format!(
                "```bash\nopenpanels-local tasks claim --task-id {task_id} --target-id <target-id> --format json\nopenpanels-local wiki markdown read --document-id {document_id} --format json\nopenpanels-local wiki pages list --wiki-space-id {wiki_space_id} --format json\nopenpanels-local wiki pages read --wiki-space-id {wiki_space_id} --path <page-path> --format json\nopenpanels-local wiki pages write --wiki-space-id {wiki_space_id} --path <page-path> --file <md-file> --task-id {task_id} --format json\nopenpanels-local tasks complete --task-id {task_id} --lease-token \"$OPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
            );
        }
        if task_type == "rebuild_wiki_index" {
            return format!(
                "```bash\nopenpanels-local tasks claim --task-id {task_id} --target-id <target-id> --format json\nopenpanels-local wiki pages list --wiki-space-id {wiki_space_id} --format json\nopenpanels-local wiki pages read --wiki-space-id {wiki_space_id} --path <page-path> --format json\nopenpanels-local wiki pages write --wiki-space-id {wiki_space_id} --path <page-path> --file <md-file> --task-id {task_id} --format json\nopenpanels-local tasks complete --task-id {task_id} --lease-token \"$OPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
            );
        }
    }
    if skill
        .metadata
        .applies_to
        .iter()
        .any(|value| value == "wiki")
    {
        return "```bash\nopenpanels-local wiki tasks next --format json\nopenpanels-local agent skill karpathy-llm-wiki --task-id <task-id> --format json\n```".to_owned();
    }
    "- No task-specific commands.".to_owned()
}

fn find_wiki_task(bootstrap: &ProjectBootstrap, task_id: &str) -> Option<Value> {
    bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .map(|snapshot| &snapshot.state)
        .unwrap_or(&bootstrap.state)
        .get("tasks")?
        .as_array()?
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .cloned()
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_guide_accepts_crlf_frontmatter() {
        let guide = parse_guide(
            "---\r\nid: test.guide\r\ntitle: Test Guide\r\n---\r\n\r\nBody.\r\n",
            "test.md",
        )
        .expect("guide");

        assert_eq!(guide.metadata.id, "test.guide");
        assert_eq!(guide.metadata.title, "Test Guide");
        assert_eq!(guide.body, "Body.\n");
    }
}
