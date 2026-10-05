//! Runtime owns authorized transport and orchestration; protocol crates stay sans-I/O.
//!
//! Ownership is explicit and singular per concern:
//!
//! - [`transport`] owns `MailTransport`, the only remote I/O seam, and its bounded
//!   line plumbing;
//! - [`pop3`] owns POP3 stream, synchronization, and body-fetch orchestration;
//! - [`smtp`] owns SMTP stream and submission orchestration;
//! - [`backend`] owns request lifecycle, results, events, and `BackendService`.
//!
//! This module is the composition and re-export surface only. An integration
//! adapter above `MailTransport` is the downstream seam; see
//! `docs/architecture/transport-boundary.md`.

mod backend;
mod pop3;
mod smtp;
mod transport;

#[cfg(test)]
mod tests;

pub use backend::{
    BackendError, BackendEvent, BackendEventKind, BackendHealth, BackendService, ContentHandle,
    CredentialSource, Credentials, DeliveryResolution, MAX_ACTIVE_CONTENT_HANDLES,
    MAX_ACTIVE_REQUESTS, MAX_CONTENT_BYTES, MAX_CONTENT_READ, MAX_RECENT_COMPLETED_REQUESTS,
    MessageSummary, OperationResult, OutboxStatus, RequestId,
};
pub use pop3::{
    SyncError, SyncReport, fetch_pop3_body, fetch_pop3_body_with_control, sync_pop3,
    sync_pop3_with_control,
};
pub use smtp::{SmtpSubmission, SubmitError, submit_smtp};
pub use transport::{ByteStream, MailTransport, OperationControl, TransportError};
