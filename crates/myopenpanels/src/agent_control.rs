use crate::control::now_iso;
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const NAMESPACE: &str = "agent_control";
const ENTRY_SKILL_REQUIREMENT_KEY: &str = "entry_skill_requirement";
const ENTRY_SKILL_ACK_PREFIX: &str = "entry_skill_ack:";

pub const ENTRY_SKILL_ID: &str = "myopenpanels";
pub const ENTRY_SKILL_VERSION: &str = env!("MYOPENPANELS_ENTRY_SKILL_VERSION");

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySkillUpdate {
    pub event_id: String,
    pub id: String,
    pub required_version: String,
    pub source: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntrySkillAcknowledgement {
    event_id: String,
    installed_version: String,
    acknowledged_at: String,
}

pub fn pending_entry_skill_update(
    paths: &MyOpenPanelsPaths,
    cli_version: &str,
) -> Result<Option<EntrySkillUpdate>, CliError> {
    let storage = Storage::open(paths)?;
    pending_entry_skill_update_with_storage(paths, &storage, cli_version)
}

pub(crate) fn pending_entry_skill_update_with_storage(
    paths: &MyOpenPanelsPaths,
    storage: &Storage,
    cli_version: &str,
) -> Result<Option<EntrySkillUpdate>, CliError> {
    let required = ensure_entry_skill_requirement(storage, cli_version)?;
    let acknowledgement = read_value::<EntrySkillAcknowledgement>(
        storage,
        &format!("{ENTRY_SKILL_ACK_PREFIX}{}", paths.context_id),
    )?;
    if acknowledgement
        .as_ref()
        .is_some_and(|acknowledgement| acknowledgement.event_id == required.event_id)
    {
        return Ok(None);
    }
    Ok(Some(required))
}

pub fn acknowledge_entry_skill_update(
    paths: &MyOpenPanelsPaths,
    event_id: &str,
    installed_version: &str,
) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let required = read_value::<EntrySkillUpdate>(&storage, ENTRY_SKILL_REQUIREMENT_KEY)?
        .ok_or_else(|| {
            CliError::with_code(
                "entry_skill_update_not_pending",
                "No MyOpenPanels Entry Skill update is pending.",
            )
        })?;
    if required.event_id != event_id {
        return Err(CliError::with_recovery(
            "entry_skill_update_changed",
            "The pending MyOpenPanels Entry Skill update changed before it was acknowledged.",
            true,
            "Run Agent Bootstrap again and follow its current Entry Skill update action.",
        ));
    }
    if !version_at_least(installed_version, &required.required_version) {
        return Err(CliError::with_recovery(
            "entry_skill_version_too_old",
            format!(
                "Entry Skill version {installed_version} does not satisfy required version {}.",
                required.required_version
            ),
            false,
            "Install the required MyOpenPanels Entry Skill, then acknowledge its installed version.",
        ));
    }
    let acknowledgement = EntrySkillAcknowledgement {
        event_id: required.event_id.clone(),
        installed_version: installed_version.to_owned(),
        acknowledged_at: now_iso(),
    };
    write_value(
        &storage,
        &format!("{ENTRY_SKILL_ACK_PREFIX}{}", paths.context_id),
        &acknowledgement,
    )?;
    Ok(json!({
        "acknowledged": true,
        "contextId": paths.context_id,
        "eventId": required.event_id,
        "installedVersion": installed_version,
        "requiredVersion": required.required_version,
    }))
}

fn ensure_entry_skill_requirement(
    storage: &Storage,
    cli_version: &str,
) -> Result<EntrySkillUpdate, CliError> {
    let source =
        format!("https://github.com/mooqii/OpenPanels/tree/v{cli_version}/skills/myopenpanels");
    if let Some(mut existing) =
        read_value::<EntrySkillUpdate>(storage, ENTRY_SKILL_REQUIREMENT_KEY)?
    {
        if version_at_least(&existing.required_version, ENTRY_SKILL_VERSION) {
            if existing.required_version == ENTRY_SKILL_VERSION && existing.source != source {
                existing.source = source;
                write_value(storage, ENTRY_SKILL_REQUIREMENT_KEY, &existing)?;
            }
            return Ok(existing);
        }
    }
    let required = EntrySkillUpdate {
        event_id: format!("entry-skill-update:{ENTRY_SKILL_VERSION}"),
        id: ENTRY_SKILL_ID.to_owned(),
        required_version: ENTRY_SKILL_VERSION.to_owned(),
        source,
        created_at: now_iso(),
    };
    write_value(storage, ENTRY_SKILL_REQUIREMENT_KEY, &required)?;
    Ok(required)
}

fn read_value<T: for<'de> Deserialize<'de>>(
    storage: &Storage,
    key: &str,
) -> Result<Option<T>, CliError> {
    let raw = storage.read_setting(NAMESPACE, key)?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(to_cli_error))
        .transpose()
}

fn write_value<T: Serialize>(storage: &Storage, key: &str, value: &T) -> Result<(), CliError> {
    let raw = serde_json::to_string(value).map_err(to_cli_error)?;
    storage.write_setting(NAMESPACE, key, &raw)
}

fn version_at_least(actual: &str, required: &str) -> bool {
    parse_skill_version(actual)
        .zip(parse_skill_version(required))
        .is_some_and(|(actual, required)| actual >= required)
}

fn parse_skill_version(value: &str) -> Option<Version> {
    let dot_count = value.chars().filter(|character| *character == '.').count();
    let normalized = match dot_count {
        0 => format!("{value}.0.0"),
        1 => format!("{value}.0"),
        _ => value.to_owned(),
    };
    Version::parse(&normalized).ok()
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_versions_compare_short_semver_values() {
        assert!(version_at_least("3.4", "3.4"));
        assert!(version_at_least("3.5", "3.4"));
        assert!(!version_at_least("3.3", "3.4"));
        assert!(!version_at_least("invalid", "3.4"));
    }
}
