use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CliError {
    code: Option<String>,
    message: String,
    exit_code: i32,
    retryable: bool,
    recovery: Option<String>,
    recovery_command: Option<String>,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            exit_code: 1,
            retryable: false,
            recovery: None,
            recovery_command: None,
        }
    }

    pub fn with_code(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: Some(code.into()),
            message: message.into(),
            exit_code: 1,
            retryable: false,
            recovery: None,
            recovery_command: None,
        }
    }

    pub fn with_recovery(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        recovery: impl Into<String>,
    ) -> Self {
        Self {
            code: Some(code.into()),
            message: message.into(),
            exit_code: 1,
            retryable,
            recovery: Some(recovery.into()),
            recovery_command: None,
        }
    }

    pub fn with_recovery_command(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        recovery: impl Into<String>,
        recovery_command: impl Into<String>,
    ) -> Self {
        Self {
            code: Some(code.into()),
            message: message.into(),
            exit_code: 1,
            retryable,
            recovery: Some(recovery.into()),
            recovery_command: Some(recovery_command.into()),
        }
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn retryable(&self) -> bool {
        self.retryable
    }

    pub fn recovery(&self) -> Option<&str> {
        self.recovery.as_deref()
    }

    pub fn recovery_command(&self) -> Option<&str> {
        self.recovery_command.as_deref()
    }
}
