use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct UserMessage {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hint: Option<String>,
}

impl UserMessage {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
        }
    }

    pub(crate) fn with_hint(
        code: &'static str,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    pub(crate) fn runtime(message: impl Into<String>) -> Self {
        Self::new("runtime.error", message)
    }

    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self::new("cli.usage", message)
    }
}

impl From<String> for UserMessage {
    fn from(message: String) -> Self {
        Self::runtime(message)
    }
}

impl From<&str> for UserMessage {
    fn from(message: &str) -> Self {
        Self::runtime(message)
    }
}

pub(crate) trait UserFacingError {
    fn user_message(&self) -> UserMessage;
}
