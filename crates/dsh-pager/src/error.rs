use std::error::Error;
use std::fmt;

use serde_json::Value;

/// Error type used at the pager's process and transport boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerError {
    message: String,
    code: Option<String>,
    details: Option<Value>,
}

impl PagerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
            details: None,
        }
    }

    /// Return the host/API error code when this error originated from an
    /// `{ ok: false, error }` response. Transport and decoding failures do not
    /// have a domain code.
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    /// Return structured host details, if the originating error supplied them.
    pub fn details(&self) -> Option<&Value> {
        self.details.as_ref()
    }
}

impl fmt::Display for PagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for PagerError {}

impl From<std::io::Error> for PagerError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for PagerError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<dsh_pager_protocol::ApiError> for PagerError {
    fn from(error: dsh_pager_protocol::ApiError) -> Self {
        Self {
            message: error.to_string(),
            code: Some(error.code),
            details: Some(error.details),
        }
    }
}

pub type PagerResult<T> = Result<T, PagerError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn api_errors_keep_code_and_details_for_diagnostic_surfaces() {
        let error = PagerError::from(dsh_pager_protocol::ApiError {
            code: "queue-item-not-found".into(),
            message: "gone".into(),
            details: json!({ "itemId": "q-1" }),
        });
        assert_eq!(error.code(), Some("queue-item-not-found"));
        assert_eq!(error.details(), Some(&json!({ "itemId": "q-1" })));
    }
}
