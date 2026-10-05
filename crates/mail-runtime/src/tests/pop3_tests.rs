//! POP3 receive, reconciliation, and credential-bound tests.
use super::fixtures::*;
use crate::*;
use i2pr_mail_domain::{ReceiveState, StoredMessageState};
use i2pr_mail_proto::pop3;
use std::sync::{Arc, Mutex};

#[test]
fn pop3_synthetic_server_syncs_header_by_opaque_uidl_and_keeps_auth_out_of_outputs() {
    let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 42\r\n+OK list\r\n1 ../uidl\r\n.\r\n+OK sizes\r\n1 42\r\n.\r\n+OK top\r\nSubject: hello\r\nFrom: a@i2p\r\n\r\n.\r\n+OK bye\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Pop3,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let report = sync_pop3(&transport, &mut store, "account", "alice", "secret").unwrap();
    assert_eq!(report.remote_count, 1);
    assert_eq!(report.headers_cached, 1);
    let stored = store
        .message_by_uidl("account", "../uidl")
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.state,
        StoredMessageState::Receive(ReceiveState::HeaderCached)
    );
    assert_eq!(
        store.raw_for_message(&stored.id).unwrap().unwrap(),
        b"Subject: hello\r\nFrom: a@i2p\r\n\r\n"
    );
    let sent = String::from_utf8(transport.written()).unwrap();
    assert!(sent.contains("USER alice\r\n"));
    assert!(sent.contains("PASS secret\r\n"));
}

#[test]
fn pop3_retr_fallback_when_top_is_rejected_caches_complete_entity() {
    let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 21\r\n+OK list\r\n1 uidl\r\n.\r\n+OK sizes\r\n1 21\r\n.\r\n-ERR TOP unsupported\r\n+OK retrieved\r\nSubject: fallback\r\n\r\nbody\r\n.\r\n+OK bye\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Pop3,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    sync_pop3(&transport, &mut store, "a", "u", "p").unwrap();
    let item = store.message_by_uidl("a", "uidl").unwrap().unwrap();
    assert_eq!(
        item.state,
        StoredMessageState::Receive(ReceiveState::BodyCached)
    );
    assert_eq!(
        store.raw_for_message(&item.id).unwrap().unwrap(),
        b"Subject: fallback\r\n\r\nbody\r\n"
    );
    assert!(
        String::from_utf8(transport.written())
            .unwrap()
            .contains("RETR 1\r\n")
    );
}

#[test]
fn pop3_delete_is_committed_only_after_quit_confirmation() {
    let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 4\r\n+OK list\r\n1 opaque\r\n.\r\n+OK sizes\r\n1 4\r\n.\r\n+OK deleted\r\n+OK bye\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Pop3,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    store
        .save_message(
            "local",
            "account",
            Some("opaque"),
            ReceiveState::HeaderCached,
            b"Subject: x\r\n\r\n",
        )
        .unwrap();
    store.mark_delete_pending("account", "opaque").unwrap();
    sync_pop3(&transport, &mut store, "account", "alice", "secret").unwrap();
    assert_eq!(
        store
            .message_by_uidl("account", "opaque")
            .unwrap()
            .unwrap()
            .state,
        StoredMessageState::Receive(ReceiveState::RemoteDeletionCommitted)
    );
    let sent = String::from_utf8(transport.written()).unwrap();
    assert!(sent.contains("DELE 1\r\n"));
    assert!(sent.contains("QUIT\r\n"));
}

#[test]
fn pop3_missing_pending_uidl_is_reconciled_as_committed() {
    let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 0 0\r\n+OK list\r\n.\r\n+OK sizes\r\n.\r\n+OK bye\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Pop3,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    store
        .save_message(
            "local",
            "account",
            Some("gone"),
            ReceiveState::HeaderCached,
            b"Subject: x\r\n\r\n",
        )
        .unwrap();
    store.mark_delete_pending("account", "gone").unwrap();
    sync_pop3(&transport, &mut store, "account", "alice", "secret").unwrap();
    assert_eq!(
        store
            .message_by_uidl("account", "gone")
            .unwrap()
            .unwrap()
            .state,
        StoredMessageState::Receive(ReceiveState::RemoteDeletionCommitted)
    );
}

#[test]
fn pop3_failed_quit_leaves_delete_reconcilable() {
    let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 4\r\n+OK list\r\n1 opaque\r\n.\r\n+OK sizes\r\n1 4\r\n.\r\n+OK deleted\r\n-ERR quit failed\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Pop3,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    store
        .save_message(
            "local",
            "account",
            Some("opaque"),
            ReceiveState::HeaderCached,
            b"Subject: x\r\n\r\n",
        )
        .unwrap();
    store.mark_delete_pending("account", "opaque").unwrap();
    assert_eq!(
        sync_pop3(&transport, &mut store, "account", "alice", "secret"),
        Err(SyncError::Protocol)
    );
    assert_eq!(
        store
            .message_by_uidl("account", "opaque")
            .unwrap()
            .unwrap()
            .state,
        StoredMessageState::Receive(ReceiveState::DeleteMarkedSession)
    );
}

#[test]
fn pop3_on_demand_body_fetch_preserves_complete_entity() {
    let transcript=b"+OK ready\r\n+OK user\r\n+OK pass\r\n+OK list\r\n1 opaque\r\n.\r\n+OK retrieved\r\nSubject: hello\r\n\r\nfull body\r\n.\r\n+OK bye\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Pop3,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    store
        .save_message(
            "local",
            "account",
            Some("opaque"),
            ReceiveState::HeaderCached,
            b"Subject: hello\r\n\r\n",
        )
        .unwrap();
    fetch_pop3_body(
        &transport, &mut store, "account", "alice", "secret", "opaque",
    )
    .unwrap();
    let item = store.message_by_uidl("account", "opaque").unwrap().unwrap();
    assert_eq!(
        item.state,
        StoredMessageState::Receive(ReceiveState::BodyCached)
    );
    assert_eq!(
        store.raw_for_message(&item.id).unwrap().unwrap(),
        b"Subject: hello\r\n\r\nfull body\r\n"
    );
}

#[test]
fn pop3_credential_ceilings_reject_before_any_command_is_written() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let transcript = b"+OK ready\r\n-ERR no cap\r\n".to_vec();
    let transport = QueueTransport(Mutex::new(std::collections::VecDeque::from([
        pop3_script(&transcript, output.clone()),
        pop3_script(&transcript, output.clone()),
    ])));
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let long_user = "a".repeat(pop3::MAX_USER_LEN + 1);
    let long_pass = "b".repeat(pop3::MAX_PASS_LEN + 1);
    assert_eq!(
        sync_pop3(&transport, &mut store, "account", &long_user, "secret"),
        Err(SyncError::Authentication)
    );
    assert_eq!(
        sync_pop3(&transport, &mut store, "account", "alice", &long_pass),
        Err(SyncError::Authentication)
    );
    let sent = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    assert!(!sent.contains("USER"), "no USER may be formatted: {sent:?}");
    assert!(!sent.contains("PASS"), "no PASS may be formatted: {sent:?}");
    assert!(!sent.contains(&long_user), "credential must not be echoed");
    assert!(!sent.contains(&long_pass), "credential must not be echoed");
    // A credential exactly at the ceiling is still accepted.
    let at_limit = "a".repeat(pop3::MAX_USER_LEN);
    assert_eq!(
        pop3::user_command(&at_limit).unwrap(),
        format!("USER {at_limit}\r\n").into_bytes()
    );
}
