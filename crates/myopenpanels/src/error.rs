use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliErrorCategory {
    Authentication,
    Conflict,
    Internal,
    NotFound,
    Precondition,
    Unavailable,
    Validation,
}

impl CliErrorCategory {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Validation => 2,
            Self::Authentication => 3,
            Self::Unavailable => 4,
            Self::Internal => 5,
            Self::Conflict | Self::NotFound | Self::Precondition => 1,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliRecoveryAction {
    pub executor: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    pub argv: Vec<String>,
}

impl CliRecoveryAction {
    pub fn cli(argv: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            executor: "cli",
            intent: None,
            argv: argv.into_iter().map(Into::into).collect(),
        }
    }

    pub fn cli_intent(
        intent: impl Into<String>,
        argv: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            executor: "cli",
            intent: Some(intent.into()),
            argv: argv.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CliError {
    category: CliErrorCategory,
    code: Option<String>,
    message: String,
    retryable: bool,
    param: Option<String>,
    recovery: Option<String>,
    recovery_actions: Vec<CliRecoveryAction>,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            category: CliErrorCategory::Internal,
            code: None,
            message: message.into(),
            retryable: false,
            param: None,
            recovery: None,
            recovery_actions: Vec::new(),
        }
    }

    pub fn with_code(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        Self {
            category: category_for_code(&code),
            code: Some(code),
            message: message.into(),
            retryable: false,
            param: None,
            recovery: None,
            recovery_actions: Vec::new(),
        }
    }

    pub fn with_recovery(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        recovery: impl Into<String>,
    ) -> Self {
        let code = code.into();
        Self {
            category: category_for_code(&code),
            code: Some(code),
            message: message.into(),
            retryable,
            param: None,
            recovery: Some(recovery.into()),
            recovery_actions: Vec::new(),
        }
    }

    pub fn with_param(mut self, param: impl Into<String>) -> Self {
        self.param = Some(param.into());
        self
    }

    pub fn with_recovery_action(mut self, action: CliRecoveryAction) -> Self {
        self.recovery_actions.push(action);
        self
    }

    pub fn category(&self) -> CliErrorCategory {
        self.category
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub fn subtype(&self) -> &str {
        self.code.as_deref().unwrap_or("command_failed")
    }

    pub fn exit_code(&self) -> i32 {
        self.category.exit_code()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn retryable(&self) -> bool {
        self.retryable
    }

    pub fn param(&self) -> Option<&str> {
        self.param.as_deref()
    }

    pub fn recovery(&self) -> Option<&str> {
        self.recovery.as_deref()
    }

    pub fn recovery_actions(&self) -> &[CliRecoveryAction] {
        &self.recovery_actions
    }
}

fn category_for_code(code: &str) -> CliErrorCategory {
    if matches!(
        code,
        "unauthorized_target" | "invalid_lease" | "lease_expired"
    ) {
        return CliErrorCategory::Authentication;
    }
    if matches!(code, "invalid_output" | "invalid_custom_skill")
        || code.ends_with("_invalid") && !code.starts_with("writing_")
    {
        return CliErrorCategory::Internal;
    }
    if code == "invalid_argument"
        || code.starts_with("invalid_")
        || code.ends_with("_file_invalid")
        || code.ends_with("_name_too_long")
        || code == "project_directory_not_found"
    {
        return CliErrorCategory::Validation;
    }
    if code == "not_found" || code.starts_with("no_current_") || code.ends_with("_not_found") {
        return CliErrorCategory::NotFound;
    }
    if code.contains("conflict")
        || code.starts_with("duplicate_")
        || code.starts_with("already_")
        || code.ends_with("_changed")
        || code.ends_with("_in_progress")
    {
        return CliErrorCategory::Conflict;
    }
    if code.ends_with("_timeout")
        || code.ends_with("_unavailable")
        || matches!(
            code,
            "browser_open_failed" | "studio_transition_busy" | "task_domain_missing"
        )
    {
        return CliErrorCategory::Unavailable;
    }
    CliErrorCategory::Precondition
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_subtypes_map_to_agent_actionable_categories_and_exit_codes() {
        let cases = [
            ("invalid_argument", CliErrorCategory::Validation, 2),
            ("task_not_found", CliErrorCategory::NotFound, 1),
            ("content_conflict", CliErrorCategory::Conflict, 1),
            ("focus_changed", CliErrorCategory::Conflict, 1),
            ("panel_kind_mismatch", CliErrorCategory::Precondition, 1),
            ("invalid_lease", CliErrorCategory::Authentication, 3),
            ("browser_open_failed", CliErrorCategory::Unavailable, 4),
            ("invalid_output", CliErrorCategory::Internal, 5),
        ];

        for (subtype, category, exit_code) in cases {
            let error = CliError::with_code(subtype, "message");
            assert_eq!(error.category(), category, "{subtype}");
            assert_eq!(error.exit_code(), exit_code, "{subtype}");
        }
    }

    #[test]
    fn recovery_actions_keep_argv_structured() {
        let error =
            CliError::with_recovery("invalid_argument", "message", false, "Read help and retry.")
                .with_param("--query")
                .with_recovery_action(CliRecoveryAction::cli(["wiki", "page", "search", "--help"]));

        assert_eq!(error.param(), Some("--query"));
        assert_eq!(
            error.recovery_actions()[0].argv,
            ["wiki", "page", "search", "--help"]
        );
    }
}
