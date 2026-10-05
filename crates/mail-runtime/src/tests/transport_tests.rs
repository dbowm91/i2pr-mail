//! Transport control, cancellation, and timeout tests.
use super::fixtures::*;
use crate::*;
use std::sync::{Arc, Mutex};

#[test]
fn transport_cancellation_and_timeout_map_to_typed_runtime_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let control = OperationControl::new(std::time::Duration::from_secs(60));
    assert_eq!(
        sync_pop3_with_control(
            &ControlledFailureTransport(TransportError::Cancelled),
            &mut store,
            "a",
            "u",
            "p",
            control
        ),
        Err(SyncError::Cancelled)
    );
    let control = OperationControl::new(std::time::Duration::from_secs(60));
    assert_eq!(
        sync_pop3_with_control(
            &ControlledFailureTransport(TransportError::Timeout),
            &mut store,
            "a",
            "u",
            "p",
            control
        ),
        Err(SyncError::Timeout)
    );
}

#[test]
fn cancelled_operation_fails_before_transport_open() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(None),
        output,
        service: i2pr_mail_domain::MailService::Pop3,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let control = OperationControl::new(std::time::Duration::from_secs(60));
    control.cancel();
    assert_eq!(
        sync_pop3_with_control(
            &transport, &mut store, "account", "alice", "secret", control
        ),
        Err(SyncError::Cancelled)
    );
}
