use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub const MAX_ID_LEN: usize = 128;
pub const MAX_UIDL_LEN: usize = 1024;
pub const MAX_DESTINATION_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("value must be non-empty ASCII printable text within its size limit")]
    InvalidValue,
    #[error("service destination must be a valid .i2p name")]
    InvalidDestination,
    #[error("invalid service port")]
    InvalidPort,
}

fn valid_token(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
                let value = value.into();
                if valid_token(&value, MAX_ID_LEN) {
                    Ok(Self(value))
                } else {
                    Err(ValidationError::InvalidValue)
                }
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $name {
            type Error = ValidationError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> String {
                value.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(AccountId);
id_type!(MessageId);
id_type!(DraftId);
id_type!(OutboxId);
id_type!(BlobId);
id_type!(MessageIdHeader);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Uidl(String);
impl Uidl {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if valid_token(&value, MAX_UIDL_LEN) {
            Ok(Self(value))
        } else {
            Err(ValidationError::InvalidValue)
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Uidl {
    type Error = ValidationError;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Self::new(v)
    }
}
impl From<Uidl> for String {
    fn from(v: Uidl) -> String {
        v.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailService {
    Pop3,
    Smtp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    destination: String,
    port: u16,
}
impl ServiceEndpoint {
    pub fn new(destination: impl Into<String>, port: u16) -> Result<Self, ValidationError> {
        let destination = destination.into();
        let valid_name = destination.is_ascii()
            && destination.len() <= MAX_DESTINATION_LEN
            && destination.ends_with(".i2p")
            && destination[..destination.len().saturating_sub(4)]
                .split('.')
                .all(|label| {
                    !label.is_empty()
                        && label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                });
        if !valid_name {
            return Err(ValidationError::InvalidDestination);
        }
        if port == 0 {
            return Err(ValidationError::InvalidPort);
        }
        Ok(Self { destination, port })
    }
    pub fn destination(&self) -> &str {
        &self.destination
    }
    pub fn port(&self) -> u16 {
        self.port
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceProfile {
    pub pop3: ServiceEndpoint,
    pub smtp: ServiceEndpoint,
}
impl Default for ServiceProfile {
    fn default() -> Self {
        Self {
            pop3: ServiceEndpoint::new("pop.postman.i2p", 110).expect("constant is valid"),
            smtp: ServiceEndpoint::new("smtp.postman.i2p", 25).expect("constant is valid"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxRole {
    Inbox,
    Drafts,
    Sent,
    Trash,
    Bulk,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiveState {
    RemoteKnown,
    HeaderCached,
    BodyCached,
    DeletePending,
    DeleteMarkedSession,
    RemoteDeletionCommitted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionState {
    Draft,
    Queued,
    Submitting,
    FailedSafeToRetry,
    DeliveryUnknown,
    Sent,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_ids_reject_empty_and_overflow() {
        assert!(AccountId::new("a").is_ok());
        assert!(AccountId::new("x".repeat(MAX_ID_LEN)).is_ok());
        assert!(AccountId::new("x".repeat(MAX_ID_LEN + 1)).is_err());
        assert!(AccountId::new("").is_err());
        macro_rules! check_id {
            ($ty:ty) => {{
                assert!(<$ty>::new("z").is_ok());
                assert!(<$ty>::new("z".repeat(MAX_ID_LEN)).is_ok());
                assert!(<$ty>::new("z".repeat(MAX_ID_LEN + 1)).is_err());
            }};
        }
        check_id!(MessageId);
        check_id!(DraftId);
        check_id!(OutboxId);
        check_id!(BlobId);
        check_id!(MessageIdHeader);
    }
    #[test]
    fn identifiers_are_distinct_and_uidl_is_opaque() {
        let uidl = Uidl::new("../opaque/uidl").unwrap();
        assert_eq!(uidl.as_str(), "../opaque/uidl");
        assert_eq!(
            MessageIdHeader::new("<a@b.i2p>").unwrap().as_str(),
            "<a@b.i2p>"
        );
        assert!(Uidl::new("x".repeat(MAX_UIDL_LEN + 1)).is_err());
    }
    #[test]
    fn service_profile_is_logical_i2p_only() {
        assert!(ServiceEndpoint::new("pop.postman.i2p", 110).is_ok());
        assert!(ServiceEndpoint::new("localhost", 110).is_err());
        assert!(ServiceEndpoint::new("x.i2p", 0).is_err());
    }
    #[test]
    fn state_vocabulary_keeps_ambiguity_explicit() {
        assert_ne!(
            SubmissionState::DeliveryUnknown,
            SubmissionState::FailedSafeToRetry
        );
    }
}
