//! SMTP submission, ambiguity, and envelope-bound tests.
use super::fixtures::*;
use crate::*;
use i2pr_mail_domain::{SubmissionProgress, SubmissionState};
use i2pr_mail_proto::smtp;
use std::sync::{Arc, Mutex};

#[test]
fn smtp_success_dot_stuffs_and_persists_sent_state_with_bcc_envelope_only() {
    let transcript=b"220 ready\r\n250-postman\r\n250-SIZE 100000\r\n250-AUTH LOGIN\r\n250 PIPELINING\r\n334 user\r\n334 pass\r\n235 auth\r\n250 sender\r\n250 recipient\r\n250 bcc\r\n354 data\r\n250 accepted\r\n221 bye\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Smtp,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let recipients = vec![
        ("to@postman.i2p".to_owned(), "To".to_owned()),
        ("blind@postman.i2p".to_owned(), "Bcc".to_owned()),
    ];
    submit_smtp(
        &transport,
        &mut store,
        SmtpSubmission {
            account_id: "account",
            outbox_id: "out-1",
            from: "from@postman.i2p",
            recipients: &recipients,
            username: "alice",
            password: "secret",
            raw: b"Subject: x\r\n\r\n.dot\r\n",
            control: OperationControl::new(std::time::Duration::from_secs(300)),
        },
    )
    .unwrap();
    assert_eq!(
        store.outbox_state("out-1").unwrap(),
        Some((SubmissionState::Sent, SubmissionProgress::DeliveryAccepted))
    );
    assert_eq!(store.outbox_recipients("out-1").unwrap(), recipients);
    assert_eq!(
        store.outbox_sender("out-1").unwrap().as_deref(),
        Some("from@postman.i2p")
    );
    assert_eq!(
        store.sent_raw("sent-out-1").unwrap().unwrap(),
        b"Subject: x\r\n\r\n.dot\r\n"
    );
    let sent = String::from_utf8(transport.written()).unwrap();
    assert!(sent.contains("..dot\r\n"));
    assert!(sent.contains("RCPT TO:<blind@postman.i2p>\r\n"));
}

#[test]
fn smtp_lost_terminal_reply_stays_delivery_unknown_and_cannot_be_claimed() {
    let transcript=b"220 ready\r\n250-postman\r\n250-SIZE 100000\r\n250-AUTH LOGIN\r\n250 PIPELINING\r\n334 user\r\n334 pass\r\n235 auth\r\n250 sender\r\n250 recipient\r\n354 data\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output,
        service: i2pr_mail_domain::MailService::Smtp,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let recipients = vec![("to@postman.i2p".to_owned(), "To".to_owned())];
    assert_eq!(
        submit_smtp(
            &transport,
            &mut store,
            SmtpSubmission {
                account_id: "account",
                outbox_id: "out-2",
                from: "from@postman.i2p",
                recipients: &recipients,
                username: "alice",
                password: "secret",
                raw: b"Subject: x\r\n\r\nbody",
                control: OperationControl::new(std::time::Duration::from_secs(300)),
            }
        ),
        Err(SubmitError::Transport)
    );
    assert_eq!(
        store.outbox_state("out-2").unwrap(),
        Some((
            SubmissionState::DeliveryUnknown,
            SubmissionProgress::DataMayHaveStarted
        ))
    );
    assert!(!store.claim_outbox("out-2").unwrap());
    assert_eq!(store.recover_submitting().unwrap(), 0);
}

#[test]
fn smtp_enforces_advertised_size_before_data_and_marks_failure_retry_safe() {
    let transcript =
        b"220 ready\r\n250-postman\r\n250-SIZE 2\r\n250-AUTH LOGIN\r\n250 done\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output: output.clone(),
        service: i2pr_mail_domain::MailService::Smtp,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let recipients = vec![("to@postman.i2p".to_owned(), "To".to_owned())];
    let result = submit_smtp(
        &transport,
        &mut store,
        SmtpSubmission {
            account_id: "a",
            outbox_id: "size",
            from: "from@postman.i2p",
            recipients: &recipients,
            username: "u",
            password: "p",
            raw: b"long body",
            control: OperationControl::new(std::time::Duration::from_secs(60)),
        },
    );
    assert_eq!(result, Err(SubmitError::SizeLimit));
    assert_eq!(
        store.outbox_state("size").unwrap(),
        Some((
            SubmissionState::FailedSafeToRetry,
            SubmissionProgress::NotStarted
        ))
    );
    assert!(
        !String::from_utf8(output.lock().unwrap().clone())
            .unwrap()
            .contains("DATA\r\n")
    );
}

#[test]
fn smtp_recipient_rejection_happens_before_data_and_remains_retry_safe() {
    let transcript=b"220 ready\r\n250-postman\r\n250-SIZE 100000\r\n250-AUTH LOGIN\r\n250 done\r\n334 user\r\n334 pass\r\n235 auth\r\n250 sender\r\n550 rejected\r\n".to_vec();
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = FakeTransport {
        script: Mutex::new(Some(Script {
            input: std::io::Cursor::new(transcript),
            output: output.clone(),
        })),
        output: output.clone(),
        service: i2pr_mail_domain::MailService::Smtp,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let recipients = vec![("bad@postman.i2p".to_owned(), "To".to_owned())];
    let result = submit_smtp(
        &transport,
        &mut store,
        SmtpSubmission {
            account_id: "a",
            outbox_id: "reject",
            from: "from@postman.i2p",
            recipients: &recipients,
            username: "u",
            password: "p",
            raw: b"safe",
            control: OperationControl::new(std::time::Duration::from_secs(60)),
        },
    );
    assert_eq!(result, Err(SubmitError::RecipientRejected));
    assert_eq!(
        store.outbox_state("reject").unwrap(),
        Some((
            SubmissionState::FailedSafeToRetry,
            SubmissionProgress::NotStarted
        ))
    );
    assert!(
        !String::from_utf8(output.lock().unwrap().clone())
            .unwrap()
            .contains("DATA\r\n")
    );
}

#[test]
fn smtp_credential_and_envelope_ceilings_reject_before_queue_or_io() {
    let output = Arc::new(Mutex::new(Vec::new()));
    let transport = QueueTransport(Mutex::new(std::collections::VecDeque::from([
        smtp_script(b"220 ready\r\n", output.clone()),
        smtp_script(b"220 ready\r\n", output.clone()),
        smtp_script(b"220 ready\r\n", output.clone()),
    ])));
    let dir = tempfile::tempdir().unwrap();
    let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
    let recipients = vec![("to@postman.i2p".to_owned(), "To".to_owned())];
    let submit = |store: &mut i2pr_mail_store::Store,
                  outbox: &str,
                  from: &str,
                  username: &str,
                  password: &str,
                  recipients: &[(String, String)]| {
        submit_smtp(
            &transport,
            store,
            SmtpSubmission {
                account_id: "account",
                outbox_id: outbox,
                from,
                recipients,
                username,
                password,
                raw: b"Subject: x\r\n\r\nbody",
                control: OperationControl::new(std::time::Duration::from_secs(60)),
            },
        )
    };
    // An over-bound username is refused before encoding or queueing.
    assert_eq!(
        submit(
            &mut store,
            "over-user",
            "from@postman.i2p",
            &"u".repeat(smtp::MAX_CREDENTIAL_LEN + 1),
            "p",
            &recipients
        ),
        Err(SubmitError::Authentication)
    );
    assert!(store.outbox_state("over-user").unwrap().is_none());
    // An over-bound recipient address cannot exceed the frozen envelope budget.
    let over_recipient = vec![("a".repeat(smtp::MAX_ENVELOPE_LINE), "To".to_owned())];
    assert_eq!(
        submit(
            &mut store,
            "over-rcpt",
            "from@postman.i2p",
            "u",
            "p",
            &over_recipient
        ),
        Err(SubmitError::RecipientRejected)
    );
    assert!(store.outbox_state("over-rcpt").unwrap().is_none());
    // An envelope address that would double-bracket is refused.
    let bracketed = vec![("<second@postman.i2p".to_owned(), "To".to_owned())];
    assert_eq!(
        submit(
            &mut store,
            "bracket-rcpt",
            "from@postman.i2p",
            "u",
            "p",
            &bracketed
        ),
        Err(SubmitError::RecipientRejected)
    );
    assert!(store.outbox_state("bracket-rcpt").unwrap().is_none());
    let sent = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    assert!(sent.is_empty(), "nothing may reach the stream: {sent:?}");
}
