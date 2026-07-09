use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CliError {
    code: Option<String>,
    message: String,
    exit_code: i32,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            exit_code: 1,
        }
    }

    pub fn with_code(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: Some(code.into()),
            message: message.into(),
            exit_code: 1,
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
}
