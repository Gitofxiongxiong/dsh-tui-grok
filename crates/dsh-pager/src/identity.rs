//! Stable, non-interchangeable identities crossing the host/UI boundary.
//!
//! The wire protocol still uses strings and integers for compatibility. These
//! wrappers make it difficult to accidentally use a row index, request id, or
//! session id as another operation's target once data reaches the adapter.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

macro_rules! number_id {
    ($name:ident, $inner:ty) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name($inner);

        impl $name {
            pub const fn new(value: $inner) -> Self {
                Self(value)
            }

            pub const fn get(self) -> $inner {
                self.0
            }
        }

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $inner {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

string_id!(DshSessionId);
string_id!(DshRequestId);
string_id!(DshQueueItemId);
string_id!(DshInteractionId);
number_id!(DshGeneration, u64);
number_id!(DshSeq, i64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_typed_and_wire_compatible() {
        let session = DshSessionId::new("session-1");
        assert_eq!(session.as_str(), "session-1");
        assert_eq!(serde_json::to_string(&DshSeq::new(4)).unwrap(), "4");
        assert_eq!(DshGeneration::new(3).get(), 3);
        assert_eq!(
            DshQueueItemId::new("q").as_str(),
            DshInteractionId::new("q").as_str()
        );
    }
}
