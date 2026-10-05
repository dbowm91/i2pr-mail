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

/// Durable progress marker for one submission attempt. Only `NotStarted` may be
/// persisted together with a retry-safe state; ambiguity is represented by
/// `DataMayHaveStarted`, which is never cleared without an explicit resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubmissionProgress {
    NotStarted,
    DataMayHaveStarted,
    DeliveryAccepted,
}

/// Durable state of one stored message row: either a receive state for remotely
/// reconciled mail, or the terminal `Sent` projection of a delivered submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredMessageState {
    Receive(ReceiveState),
    Sent,
}

impl ReceiveState {
    pub const ALL: [Self; 6] = [
        Self::RemoteKnown,
        Self::HeaderCached,
        Self::BodyCached,
        Self::DeletePending,
        Self::DeleteMarkedSession,
        Self::RemoteDeletionCommitted,
    ];
    /// Canonical storage token. Storage encoding is owned here so persisted
    /// vocabulary cannot drift from the Rust type.
    pub fn as_storage_str(self) -> &'static str {
        match self {
            Self::RemoteKnown => "RemoteKnown",
            Self::HeaderCached => "HeaderCached",
            Self::BodyCached => "BodyCached",
            Self::DeletePending => "DeletePending",
            Self::DeleteMarkedSession => "DeleteMarkedSession",
            Self::RemoteDeletionCommitted => "RemoteDeletionCommitted",
        }
    }
    pub fn from_storage_str(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_storage_str() == value)
    }
}

impl SubmissionState {
    pub const ALL: [Self; 6] = [
        Self::Draft,
        Self::Queued,
        Self::Submitting,
        Self::FailedSafeToRetry,
        Self::DeliveryUnknown,
        Self::Sent,
    ];
    pub fn as_storage_str(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Queued => "Queued",
            Self::Submitting => "Submitting",
            Self::FailedSafeToRetry => "FailedSafeToRetry",
            Self::DeliveryUnknown => "DeliveryUnknown",
            Self::Sent => "Sent",
        }
    }
    pub fn from_storage_str(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_storage_str() == value)
    }
    /// The only progress markers that may be persisted with this state. Any other
    /// pairing is an unrepresentable combination rather than a runtime decision.
    pub fn allowed_progress(self) -> &'static [SubmissionProgress] {
        use SubmissionProgress::{DataMayHaveStarted, DeliveryAccepted, NotStarted};
        match self {
            Self::Draft | Self::Queued | Self::FailedSafeToRetry => &[NotStarted],
            Self::Submitting => &[NotStarted, DataMayHaveStarted],
            Self::DeliveryUnknown => &[DataMayHaveStarted],
            Self::Sent => &[DeliveryAccepted],
        }
    }
    pub fn allows(self, progress: SubmissionProgress) -> bool {
        self.allowed_progress().contains(&progress)
    }
}

impl SubmissionProgress {
    pub const ALL: [Self; 3] = [
        Self::NotStarted,
        Self::DataMayHaveStarted,
        Self::DeliveryAccepted,
    ];
    pub fn as_storage_i64(self) -> i64 {
        match self {
            Self::NotStarted => 0,
            Self::DataMayHaveStarted => 1,
            Self::DeliveryAccepted => 2,
        }
    }
    pub fn from_storage_i64(value: i64) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|progress| progress.as_storage_i64() == value)
    }
}

impl StoredMessageState {
    pub fn receive_state(self) -> Option<ReceiveState> {
        match self {
            Self::Receive(state) => Some(state),
            Self::Sent => None,
        }
    }
    pub fn as_storage_str(self) -> &'static str {
        match self {
            Self::Receive(state) => state.as_storage_str(),
            Self::Sent => "Sent",
        }
    }
    pub fn from_storage_str(value: &str) -> Option<Self> {
        match value {
            "Sent" => Some(Self::Sent),
            _ => ReceiveState::from_storage_str(value).map(Self::Receive),
        }
    }
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

    #[test]
    fn storage_vocabulary_is_total_bounded_and_strict() {
        for state in ReceiveState::ALL {
            assert_eq!(
                ReceiveState::from_storage_str(state.as_storage_str()),
                Some(state)
            );
        }
        for state in SubmissionState::ALL {
            assert_eq!(
                SubmissionState::from_storage_str(state.as_storage_str()),
                Some(state)
            );
        }
        for progress in SubmissionProgress::ALL {
            assert_eq!(
                SubmissionProgress::from_storage_i64(progress.as_storage_i64()),
                Some(progress)
            );
        }
        assert_eq!(ReceiveState::from_storage_str("Sent"), None);
        assert_eq!(ReceiveState::from_storage_str(""), None);
        assert_eq!(ReceiveState::from_storage_str("remoteknown"), None);
        assert_eq!(SubmissionState::from_storage_str("Unknown"), None);
        assert_eq!(SubmissionProgress::from_storage_i64(3), None);
        assert_eq!(SubmissionProgress::from_storage_i64(-1), None);
        assert_eq!(
            StoredMessageState::from_storage_str("Sent"),
            Some(StoredMessageState::Sent)
        );
        assert_eq!(StoredMessageState::Sent.receive_state(), None);
    }

    #[test]
    fn submission_state_progress_combinations_are_representable_only() {
        use SubmissionProgress::*;
        for state in SubmissionState::ALL {
            for progress in SubmissionProgress::ALL {
                let represented = state.allows(progress);
                assert_eq!(
                    represented,
                    state.allowed_progress().contains(&progress),
                    "{state:?}/{progress:?}"
                );
            }
        }
        assert!(SubmissionState::DeliveryUnknown.allows(DataMayHaveStarted));
        assert!(!SubmissionState::DeliveryUnknown.allows(NotStarted));
        assert!(!SubmissionState::Sent.allows(DataMayHaveStarted));
        assert!(SubmissionState::Submitting.allows(NotStarted));
        assert!(!SubmissionState::Queued.allows(DataMayHaveStarted));
        assert!(!SubmissionState::FailedSafeToRetry.allows(DeliveryAccepted));
    }
}
