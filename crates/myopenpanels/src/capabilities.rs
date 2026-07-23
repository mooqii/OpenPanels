use crate::error::CliError;
use crate::types::PanelKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

static MODULE_CAPABILITY_CATALOG_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../agent-resources/module-capability-catalog.json"
));
static MODULE_CAPABILITY_CATALOG: OnceLock<Result<CapabilityCatalog, String>> = OnceLock::new();
static MODULE_CATALOG_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../agent-resources/module-catalog.json"
));

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapabilityCatalog {
    pub capabilities: Vec<CapabilityDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapabilityDefinition {
    pub key: String,
    pub description: String,
    pub panel_kind: Option<String>,
    pub platform_contract: PlatformContract,
    pub local_skill: LocalSkillPolicy,
    pub invocation: CapabilityInvocation,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PlatformContract {
    pub system_skill_id: String,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LocalSkillPolicy {
    pub mode: String,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub task_pointer: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum CapabilityInvocation {
    Procedure {
        selection_policy: String,
        command_intents: Vec<String>,
    },
    Task {
        routes: Vec<TaskRoute>,
    },
    TaskScope {
        scope_kinds: Vec<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TaskRoute {
    pub queue: String,
    pub task_type: String,
    pub capability: String,
    pub handler_key: String,
}

pub(crate) fn capability_catalog() -> Result<&'static CapabilityCatalog, CliError> {
    MODULE_CAPABILITY_CATALOG
        .get_or_init(|| parse_capability_catalog().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| CliError::with_code("capability_catalog_invalid", error.clone()))
}

pub(crate) fn module_catalog() -> Result<serde_json::Value, CliError> {
    serde_json::from_str(MODULE_CATALOG_SOURCE).map_err(to_cli_error)
}

pub(crate) fn command_domains_for_panel(panel_kind: PanelKind) -> Result<Vec<String>, CliError> {
    let catalog = module_catalog()?;
    let modules = catalog
        .get("modules")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            CliError::with_code(
                "module_catalog_invalid",
                "Module Catalog must contain a modules array.",
            )
        })?;
    let mut domains = BTreeSet::new();
    for module in modules {
        let is_panel_consumer = module
            .get("panelKinds")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|panel_kinds| {
                panel_kinds
                    .iter()
                    .any(|kind| kind.as_str() == Some(panel_kind.as_str()))
            });
        if !is_panel_consumer {
            continue;
        }
        let Some(module_key) = module.get("key").and_then(serde_json::Value::as_str) else {
            return Err(CliError::with_code(
                "module_catalog_invalid",
                "Every Module Catalog entry must have a key.",
            ));
        };
        let command_domain = match module_key {
            "canvas-document" => "canvas",
            "wiki-space" => "wiki",
            other => other,
        };
        if crate::cli::registry::catalog(Some(command_domain)).is_some() {
            domains.insert(command_domain.to_owned());
        }
    }
    Ok(domains.into_iter().collect())
}

pub(crate) fn module_key_for_capability(key: &str) -> Option<&str> {
    if key.starts_with("panel.") {
        return None;
    }
    key.split('.').next().map(|module| match module {
        "canvas" => "canvas-document",
        other => other,
    })
}

pub(crate) fn capability(key: &str) -> Result<Option<&'static CapabilityDefinition>, CliError> {
    Ok(capability_catalog()?
        .capabilities
        .iter()
        .find(|capability| capability.key == key))
}

pub(crate) fn task_route(
    queue: &str,
    task_type: &str,
    task_capability: &str,
) -> Result<Option<&'static TaskRoute>, CliError> {
    Ok(
        task_capability_for_route(queue, task_type, task_capability)?.and_then(|definition| {
            let CapabilityInvocation::Task { routes } = &definition.invocation else {
                return None;
            };
            routes.iter().find(|route| {
                route.queue == queue
                    && route.task_type == task_type
                    && route.capability == task_capability
            })
        }),
    )
}

pub(crate) fn task_capability_for_route(
    queue: &str,
    task_type: &str,
    task_capability: &str,
) -> Result<Option<&'static CapabilityDefinition>, CliError> {
    Ok(capability_catalog()?
        .capabilities
        .iter()
        .find(|definition| {
            let CapabilityInvocation::Task { routes } = &definition.invocation else {
                return false;
            };
            routes.iter().any(|route| {
                route.queue == queue
                    && route.task_type == task_type
                    && route.capability == task_capability
            })
        }))
}

pub(crate) fn task_route_for_capability(
    capability_key: &str,
    task_type: &str,
) -> Result<&'static TaskRoute, CliError> {
    let definition = capability(capability_key)?.ok_or_else(|| {
        CliError::with_code(
            "task_capability_not_found",
            format!("Task Capability not found: {capability_key}"),
        )
    })?;
    let CapabilityInvocation::Task { routes } = &definition.invocation else {
        return Err(CliError::with_code(
            "task_capability_invalid",
            format!("Capability {capability_key} is not a Task Capability."),
        ));
    };
    routes
        .iter()
        .find(|route| route.task_type == task_type)
        .ok_or_else(|| {
            CliError::with_code(
                "task_route_not_found",
                format!("Task Capability {capability_key} has no route for type {task_type}."),
            )
        })
}

pub(crate) fn task_route_for_queue_and_type(
    queue: &str,
    task_type: &str,
) -> Result<&'static TaskRoute, CliError> {
    let mut matches =
        task_routes()?.filter(|route| route.queue == queue && route.task_type == task_type);
    let route = matches.next().ok_or_else(|| {
        CliError::with_code(
            "task_route_not_found",
            format!("No Task Capability route is registered for {queue}/{task_type}."),
        )
    })?;
    if matches.next().is_some() {
        return Err(CliError::with_code(
            "task_route_ambiguous",
            format!("Multiple Task Capability routes are registered for {queue}/{task_type}."),
        ));
    }
    Ok(route)
}

pub(crate) fn task_route_for_handler(
    handler_key: &str,
) -> Result<Option<&'static TaskRoute>, CliError> {
    Ok(task_routes()?.find(|route| route.handler_key == handler_key))
}

pub(crate) fn task_routes() -> Result<impl Iterator<Item = &'static TaskRoute>, CliError> {
    Ok(capability_catalog()?
        .capabilities
        .iter()
        .filter_map(|capability| match &capability.invocation {
            CapabilityInvocation::Task { routes } => Some(routes.as_slice()),
            _ => None,
        })
        .flatten())
}

pub(crate) fn validate_task_local_skill(
    queue: &str,
    task_type: &str,
    task_capability: &str,
    input: &serde_json::Value,
    source: &serde_json::Value,
) -> Result<(), CliError> {
    let definition =
        task_capability_for_route(queue, task_type, task_capability)?.ok_or_else(|| {
            CliError::with_code(
                "task_route_not_found",
                format!(
                "No Task Capability route is registered for {queue}/{task_type}/{task_capability}."
            ),
            )
        })?;
    if !matches!(definition.local_skill.mode.as_str(), "required" | "fixed") {
        return Ok(());
    }
    let pointer = definition
        .local_skill
        .task_pointer
        .as_deref()
        .expect("validated Task Local Skill policy");
    let task = serde_json::json!({ "input": input, "source": source });
    if task.pointer(pointer).is_some_and(meaningful_json_value) {
        return Ok(());
    }
    Err(CliError::with_code(
        "local_skill_required",
        format!(
            "Task Capability {} requires a captured Local Skill at {pointer}.",
            definition.key
        ),
    ))
}

fn parse_capability_catalog() -> Result<CapabilityCatalog, CliError> {
    let catalog: CapabilityCatalog =
        serde_json::from_str(MODULE_CAPABILITY_CATALOG_SOURCE).map_err(to_cli_error)?;
    validate_capability_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_capability_catalog(catalog: &CapabilityCatalog) -> Result<(), CliError> {
    let mut keys = BTreeSet::new();
    let mut routes = BTreeSet::new();
    for capability in &catalog.capabilities {
        if capability.key.trim().is_empty()
            || capability.description.trim().is_empty()
            || !keys.insert(capability.key.as_str())
        {
            return Err(invalid_capability(
                capability,
                "has an invalid or duplicate key",
            ));
        }
        if capability
            .panel_kind
            .as_deref()
            .is_some_and(|kind| PanelKind::parse(kind).is_none())
        {
            return Err(invalid_capability(capability, "has an invalid panel kind"));
        }
        validate_platform_contract(capability)?;
        validate_local_skill_policy(capability)?;
        match &capability.invocation {
            CapabilityInvocation::Procedure {
                selection_policy,
                command_intents,
            } => {
                if !matches!(
                    selection_policy.as_str(),
                    "none" | "summary" | "optional-detail" | "active-detail" | "explicit-detail"
                ) || !nonempty_unique(command_intents)
                {
                    return Err(invalid_capability(
                        capability,
                        "has an invalid Procedure invocation",
                    ));
                }
            }
            CapabilityInvocation::Task {
                routes: task_routes,
            } => {
                if task_routes.is_empty() {
                    return Err(invalid_capability(capability, "has no Task routes"));
                }
                let mut task_types = BTreeSet::new();
                for route in task_routes {
                    if route.queue.trim().is_empty()
                        || route.task_type.trim().is_empty()
                        || route.capability.trim().is_empty()
                        || route.handler_key.trim().is_empty()
                        || !task_types.insert(route.task_type.as_str())
                        || !routes.insert((
                            route.queue.as_str(),
                            route.task_type.as_str(),
                            route.capability.as_str(),
                        ))
                    {
                        return Err(invalid_capability(
                            capability,
                            "has an invalid or duplicate Task route",
                        ));
                    }
                }
            }
            CapabilityInvocation::TaskScope { scope_kinds } => {
                if !nonempty_unique(scope_kinds)
                    || scope_kinds.iter().any(|kind| {
                        !matches!(
                            kind.as_str(),
                            "exact-task" | "project-drain" | "wiki-mutation-drain"
                        )
                    })
                {
                    return Err(invalid_capability(
                        capability,
                        "has invalid Task scope kinds",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_platform_contract(capability: &CapabilityDefinition) -> Result<(), CliError> {
    let contract = &capability.platform_contract;
    if contract.system_skill_id.trim().is_empty() || contract.references.is_empty() {
        return Err(invalid_capability(
            capability,
            "has an incomplete platform contract",
        ));
    }
    let mut references = BTreeSet::new();
    for reference in &contract.references {
        let path = Path::new(reference);
        if reference.trim().is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || !references.insert(reference)
        {
            return Err(invalid_capability(
                capability,
                "has an invalid or duplicate platform reference",
            ));
        }
    }
    Ok(())
}

fn validate_local_skill_policy(capability: &CapabilityDefinition) -> Result<(), CliError> {
    let policy = &capability.local_skill;
    let task_pointer_valid = policy
        .task_pointer
        .as_deref()
        .is_some_and(|pointer| pointer.starts_with("/input/") || pointer.starts_with("/source/"));
    let valid = match (&capability.invocation, policy.mode.as_str()) {
        (CapabilityInvocation::Task { .. }, "none") => {
            policy.skill_id.is_none() && policy.task_pointer.is_none()
        }
        (CapabilityInvocation::Task { .. }, "optional" | "required") => {
            policy.skill_id.is_none() && task_pointer_valid
        }
        (CapabilityInvocation::Task { .. }, "fixed") => {
            policy
                .skill_id
                .as_deref()
                .is_some_and(|skill_id| !skill_id.trim().is_empty())
                && task_pointer_valid
        }
        (CapabilityInvocation::TaskScope { .. }, "none") => {
            policy.skill_id.is_none() && policy.task_pointer.is_none()
        }
        (CapabilityInvocation::Procedure { .. }, "none" | "optional" | "required") => {
            policy.skill_id.is_none() && policy.task_pointer.is_none()
        }
        (CapabilityInvocation::Procedure { .. }, "fixed") => {
            policy
                .skill_id
                .as_deref()
                .is_some_and(|skill_id| !skill_id.trim().is_empty())
                && policy.task_pointer.is_none()
        }
        _ => false,
    };
    if !valid {
        return Err(invalid_capability(
            capability,
            "has an invalid Local Skill policy",
        ));
    }
    Ok(())
}

fn meaningful_json_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::String(value) => !value.trim().is_empty(),
        serde_json::Value::Array(value) => !value.is_empty(),
        serde_json::Value::Object(value) => !value.is_empty(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
    }
}

fn nonempty_unique(values: &[String]) -> bool {
    !values.is_empty()
        && values.iter().all(|value| !value.trim().is_empty())
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn invalid_capability(capability: &CapabilityDefinition, message: &str) -> CliError {
    CliError::with_code(
        "capability_catalog_invalid",
        format!("Capability {} {message}.", capability.key),
    )
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_owns_typed_invocations_routes_and_local_skill_policy() {
        let catalog = capability_catalog().expect("Capability Catalog");
        assert_eq!(catalog.capabilities.len(), 28);
        let procedure_count = catalog
            .capabilities
            .iter()
            .filter(|capability| {
                matches!(
                    &capability.invocation,
                    CapabilityInvocation::Procedure { .. }
                )
            })
            .count();
        assert_eq!(procedure_count, 19);
        assert!(catalog.capabilities.iter().all(|capability| {
            !matches!(
                &capability.invocation,
                CapabilityInvocation::Procedure { .. }
            ) || capability.local_skill.mode == "none"
        }));
        assert_eq!(
            catalog
                .capabilities
                .iter()
                .filter(|capability| capability.local_skill.mode == "required")
                .count(),
            7
        );
        assert_eq!(task_routes().expect("Task routes").count(), 10);
    }

    #[test]
    fn task_route_resolution_uses_the_panel_capability_key() {
        let route = task_route_for_capability("release.execute", "release_xiaohongshu")
            .expect("Publishing route");
        assert_eq!(route.queue, "release");
        assert_eq!(route.capability, "release.xiaohongshu");
        assert_eq!(route.handler_key, "handler.release.xiaohongshu");
    }

    #[test]
    fn required_task_local_skill_is_enforced_from_the_catalog_pointer() {
        let missing = validate_task_local_skill(
            "release",
            "release_xiaohongshu",
            "release.xiaohongshu",
            &serde_json::json!({}),
            &serde_json::json!({}),
        )
        .expect_err("missing Publishing Skill");
        assert_eq!(missing.code(), Some("local_skill_required"));

        validate_task_local_skill(
            "release",
            "release_xiaohongshu",
            "release.xiaohongshu",
            &serde_json::json!({ "publishingSkillSnapshot": { "id": "release-xiaohongshu" } }),
            &serde_json::json!({}),
        )
        .expect("captured Publishing Skill");
    }
}
