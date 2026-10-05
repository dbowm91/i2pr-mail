//! Backend service workflow, request-ledger, and release tests.
use super::fixtures::*;
use crate::backend::{LedgerError, RequestLedger};
use crate::*;
use i2pr_mail_domain::{StoredMessageState, SubmissionProgress, SubmissionState};
use std::sync::{Arc, Mutex};

#[test]
fn request_ledger_rejects_duplicates_and_bounds_only_live_capacity() {
    let mut ledger = RequestLedger::default();
    let id = RequestId::new("req-1").unwrap();
    assert_eq!(ledger.begin(&id), Ok(()));
    // A duplicate while the request is still active is rejected.
    assert_eq!(ledger.begin(&id), Err(LedgerError::Duplicate));
    ledger.end(&id);
    assert_eq!(ledger.active_len(), 0);
    // A duplicate inside the recent-completion window is still rejected.
    assert_eq!(ledger.begin(&id), Err(LedgerError::Duplicate));
    assert_eq!(ledger.completed_len(), 1);

    // In-flight ownership is a genuinely live ceiling.
    let mut active = Vec::new();
    for n in 0..MAX_ACTIVE_REQUESTS {
        let next = RequestId::new(format!("active-{n}")).unwrap();
        assert_eq!(ledger.begin(&next), Ok(()));
        active.push(next);
    }
    assert_eq!(
        ledger.begin(&RequestId::new("overflow").unwrap()),
        Err(LedgerError::Capacity)
    );
    for id in &active {
        ledger.end(id);
    }
    assert_eq!(ledger.active_len(), 0);
}

#[test]
fn recent_completion_window_evicts_instead_of_failing() {
    let mut ledger = RequestLedger::default();
    let total = MAX_RECENT_COMPLETED_REQUESTS * 2 + 17;
    for n in 0..total {
        let id = RequestId::new(format!("req-{n}")).unwrap();
        ledger.begin(&id).unwrap();
        ledger.end(&id);
    }
    // Retention stays bounded rather than growing with service lifetime.
    assert_eq!(ledger.active_len(), 0);
    assert_eq!(ledger.completed_len(), MAX_RECENT_COMPLETED_REQUESTS);
    // An evicted id may be reused because no durable idempotency is claimed.
    assert_eq!(
        ledger.begin(&RequestId::new("req-0").unwrap()),
        Ok(()),
        "oldest completion should have been evicted"
    );
    // The most recent completion is still deduplicated.
    assert_eq!(
        ledger.begin(&RequestId::new(format!("req-{}", total - 1)).unwrap()),
        Err(LedgerError::Duplicate)
    );
}

#[test]
fn sustained_backend_use_recovers_request_capacity_without_restart() {
    let dir = tempfile::tempdir().unwrap();
    let offline = Arc::new(QueueTransport(Mutex::new(Default::default())));
    let mut service = BackendService::open(dir.path(), "account", offline).unwrap();
    let total = MAX_RECENT_COMPLETED_REQUESTS * 2 + 17;
    for n in 0..total {
        let outcome = service.health(RequestId::new(format!("req-{n}")).unwrap());
        assert!(
            outcome.value.is_ok(),
            "request {n} of {total} must not exhaust request capacity"
        );
    }
    let health = service
        .health(RequestId::new("sustained-final").unwrap())
        .value
        .unwrap();
    // Only the in-flight probe itself is active; nothing leaked.
    assert_eq!(health.active_requests, 1);
    assert_eq!(health.retained_request_ids, MAX_RECENT_COMPLETED_REQUESTS);
    // An id evicted long ago is reusable.
    assert!(
        service
            .health(RequestId::new("req-0").unwrap())
            .value
            .is_ok()
    );
    // A recent completion is still deduplicated.
    assert_eq!(
        service
            .health(RequestId::new("sustained-final").unwrap())
            .value,
        Err(BackendError::InvalidRequest)
    );
    let newest = RequestId::new(format!("req-{}", total - 1)).unwrap();
    assert_eq!(
        service.health(newest).value,
        Err(BackendError::InvalidRequest)
    );
}

#[test]
fn failing_and_cancelled_requests_release_active_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let offline = Arc::new(QueueTransport(Mutex::new(Default::default())));
    let mut service = BackendService::open(dir.path(), "account", offline).unwrap();
    for n in 0..MAX_ACTIVE_REQUESTS * 2 {
        assert_eq!(
            service
                .open_message(RequestId::new(format!("fail-{n}")).unwrap(), "missing")
                .value,
            Err(BackendError::NotFound)
        );
    }
    // Credential failure, cancellation, and an unknown outbox row all terminate.
    let cancelled = OperationControl::new(std::time::Duration::from_secs(60));
    cancelled.cancel();
    assert_eq!(
        service
            .sync(
                RequestId::new("cancelled-sync").unwrap(),
                &TestCredentials,
                cancelled
            )
            .value,
        Err(BackendError::Cancelled)
    );
    assert_eq!(
        service
            .sync(
                RequestId::new("credential-sync").unwrap(),
                &FailingCredentials,
                OperationControl::new(std::time::Duration::from_secs(60))
            )
            .value,
        Err(BackendError::Authentication)
    );
    assert_eq!(
        service
            .send_queued(
                RequestId::new("missing-outbox").unwrap(),
                "no-such-outbox",
                &TestCredentials,
                OperationControl::new(std::time::Duration::from_secs(60))
            )
            .value,
        Err(BackendError::NotFound)
    );
    let health = service
        .health(RequestId::new("leak-probe").unwrap())
        .value
        .unwrap();
    assert_eq!(
        health.active_requests, 1,
        "terminal paths must release ownership"
    );
    // Retained history stays bounded no matter how many requests failed.
    assert!(health.retained_request_ids <= MAX_RECENT_COMPLETED_REQUESTS);
}

#[test]
fn backend_service_runs_fake_mail_workflow_restarts_offline_and_bounds_handles() {
    let pop3=b"+OK ready\r\n+OK capabilities\r\nTOP\r\nUIDL\r\n.\r\n+OK user\r\n+OK pass\r\n+OK 1 22\r\n+OK uidls\r\n1 opaque\r\n.\r\n+OK list\r\n1 22\r\n.\r\n+OK top\r\nSubject: received\r\n\r\n.\r\n+OK bye\r\n".to_vec();
    let smtp=b"220 ready\r\n250-postman\r\n250-SIZE 100000\r\n250-AUTH LOGIN\r\n250 done\r\n334 user\r\n334 pass\r\n235 auth\r\n250 sender\r\n250 recipient\r\n354 data\r\n250 accepted\r\n221 bye\r\n".to_vec();
    let scripts = std::collections::VecDeque::from([
        (
            i2pr_mail_domain::MailService::Pop3,
            Script {
                input: std::io::Cursor::new(pop3),
                output: Arc::new(Mutex::new(Vec::new())),
            },
        ),
        (
            i2pr_mail_domain::MailService::Smtp,
            Script {
                input: std::io::Cursor::new(smtp),
                output: Arc::new(Mutex::new(Vec::new())),
            },
        ),
    ]);
    let transport = Arc::new(QueueTransport(Mutex::new(scripts)));
    let dir = tempfile::tempdir().unwrap();
    let mut service = BackendService::open(dir.path(), "account", transport).unwrap();
    let sync = service.sync(
        RequestId::new("req-sync").unwrap(),
        &TestCredentials,
        OperationControl::new(std::time::Duration::from_secs(60)),
    );
    assert_eq!(sync.value.unwrap().headers_cached, 1);
    assert_eq!(sync.events.len(), 2);
    let listed = service
        .list_messages(RequestId::new("req-list").unwrap())
        .value
        .unwrap();
    let incoming = listed
        .iter()
        .find(|m| m.uidl.as_deref() == Some("opaque"))
        .unwrap();
    let handle = service
        .open_message(RequestId::new("req-open").unwrap(), &incoming.id)
        .value
        .unwrap();
    assert_eq!(
        service
            .read_content(RequestId::new("req-read").unwrap(), handle, 0, 100)
            .value
            .unwrap(),
        b"Subject: received\r\n\r\n"
    );
    service
        .save_draft(
            RequestId::new("req-draft").unwrap(),
            "draft-1",
            b"draft entity",
        )
        .value
        .unwrap();
    service
        .delete_draft(RequestId::new("req-draft-delete").unwrap(), "draft-1")
        .value
        .unwrap();
    let recipients = vec![("to@postman.i2p".to_owned(), "To".to_owned())];
    service
        .queue_send(
            RequestId::new("req-queue").unwrap(),
            "out-1",
            "from@postman.i2p",
            &recipients,
            b"Subject: send\r\n\r\nbody",
        )
        .value
        .unwrap();
    service
        .send_queued(
            RequestId::new("req-send").unwrap(),
            "out-1",
            &TestCredentials,
            OperationControl::new(std::time::Duration::from_secs(60)),
        )
        .value
        .unwrap();
    assert_eq!(
        service
            .inspect_outbox(RequestId::new("req-outbox").unwrap(), "out-1")
            .value
            .unwrap()
            .state,
        SubmissionState::Sent
    );
    assert!(
        service
            .health(RequestId::new("req-health").unwrap())
            .value
            .unwrap()
            .ready
    );
    assert_eq!(
        service.service_profile().pop3.destination(),
        "pop.postman.i2p"
    );
    let listed = service
        .list_messages(RequestId::new("req-list-2").unwrap())
        .value
        .unwrap();
    assert!(listed.iter().any(|m| m.state == StoredMessageState::Sent));
    let incoming_id = listed
        .iter()
        .find(|m| m.uidl.as_deref() == Some("opaque"))
        .unwrap()
        .id
        .clone();
    service
        .delete_local(RequestId::new("req-local-delete").unwrap(), &incoming_id)
        .value
        .unwrap();
    let after_delete = service
        .list_messages(RequestId::new("req-list-deleted").unwrap())
        .value
        .unwrap();
    assert!(
        after_delete
            .iter()
            .find(|m| m.id == incoming_id)
            .unwrap()
            .locally_deleted
    );
    assert_eq!(
        service
            .list_messages(RequestId::new("req-list").unwrap())
            .value,
        Err(BackendError::InvalidRequest)
    );
    assert_eq!(
        service
            .read_content(
                RequestId::new("req-too-large").unwrap(),
                handle,
                0,
                MAX_CONTENT_READ + 1
            )
            .value,
        Err(BackendError::Capacity)
    );
    service
        .release_content(RequestId::new("req-close").unwrap(), handle)
        .value
        .unwrap();
    assert_eq!(
        service
            .read_content(RequestId::new("req-stale-handle").unwrap(), handle, 0, 10)
            .value,
        Err(BackendError::NotFound)
    );
    service.shutdown();

    {
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        let recipients = vec![("pending@postman.i2p".to_owned(), "To".to_owned())];
        store
            .queue_outbox_full(
                "out-unknown",
                "from@postman.i2p",
                b"uncertain entity",
                &recipients,
            )
            .unwrap();
        store.claim_outbox("out-unknown").unwrap();
        store
            .set_outbox_stage(
                "out-unknown",
                SubmissionProgress::DataMayHaveStarted,
                SubmissionState::Submitting,
            )
            .unwrap();
    }
    let offline = Arc::new(QueueTransport(Mutex::new(Default::default())));
    let mut restarted = BackendService::open(dir.path(), "account", offline).unwrap();
    let listed = restarted
        .list_messages(RequestId::new("req-offline").unwrap())
        .value
        .unwrap();
    assert!(listed.iter().any(|m| m.state == StoredMessageState::Sent));
    assert_eq!(
        restarted
            .inspect_outbox(RequestId::new("req-unknown").unwrap(), "out-unknown")
            .value
            .unwrap()
            .state,
        SubmissionState::DeliveryUnknown
    );
    assert_eq!(
        restarted
            .send_queued(
                RequestId::new("req-no-retry").unwrap(),
                "out-unknown",
                &TestCredentials,
                OperationControl::new(std::time::Duration::from_secs(60))
            )
            .value,
        Err(BackendError::DeliveryUnknown)
    );
    restarted
        .resolve_delivery_unknown(
            RequestId::new("req-resolve-unknown").unwrap(),
            "out-unknown",
            DeliveryResolution::PermitRetry,
        )
        .value
        .unwrap();
    assert_eq!(
        restarted
            .inspect_outbox(
                RequestId::new("req-resolved-status").unwrap(),
                "out-unknown"
            )
            .value
            .unwrap()
            .state,
        SubmissionState::FailedSafeToRetry
    );
    let received = listed
        .iter()
        .find(|m| m.uidl.as_deref() == Some("opaque"))
        .unwrap();
    assert!(received.locally_deleted);
    let inbox_handle = restarted
        .open_message(
            RequestId::new("req-open-inbox-offline").unwrap(),
            &received.id,
        )
        .value
        .unwrap();
    assert_eq!(
        restarted
            .read_content(
                RequestId::new("req-read-inbox-offline").unwrap(),
                inbox_handle,
                0,
                100
            )
            .value
            .unwrap(),
        b"Subject: received\r\n\r\n"
    );
    let sent = listed
        .iter()
        .find(|m| m.state == StoredMessageState::Sent)
        .unwrap();
    let handle = restarted
        .open_message(RequestId::new("req-open-sent").unwrap(), &sent.id)
        .value
        .unwrap();
    assert_eq!(
        restarted
            .read_content(RequestId::new("req-read-sent").unwrap(), handle, 0, 100)
            .value
            .unwrap(),
        b"Subject: send\r\n\r\nbody"
    );
}
