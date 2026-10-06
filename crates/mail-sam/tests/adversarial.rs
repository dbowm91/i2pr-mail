//! Cross-cutting adversarial properties of the SAM 3.1 codec.
//!
//! These are deliberately integration tests rather than unit tests inside the
//! modules: they exercise the crate exactly as an adapter would consume it, from
//! outside, and they assert the *properties* the security argument rests on
//! rather than re-deriving each function's contract.
//!
//! The properties, in one sentence each:
//!
//! 1. A caller-supplied value can never add a SAM token or split a command line.
//!    SAM tokenizes on whitespace and then on the first `=`, so the only bytes
//!    that cross a token boundary are space, `"`, `\`, control bytes and non-ASCII.
//!    `=` alone is therefore safe and must stay accepted — rejecting it would be
//!    a false positive, not a defence.
//! 2. A generated line is exactly what the router expects: no doubled spaces
//!    (a `KEY= VALUE` pair reads as a malformed option), one trailing newline,
//!    and no interior newline.
//! 3. Both documented shapes of a `STREAM CONNECT` success reply normalize to one
//!    typed value, and an unanchored `SAM OK` is refused.
//! 4. Bounds are enforced exactly and never truncate. `MAX_LINE` is an outer
//!    backstop on the reply side; the per-value bounds are stricter and fire first.
//! 5. An unknown result token, an unknown reply shape, and a missing value fail
//!    closed instead of being coerced into success.

use i2pr_mail_sam::client::{
    MAX_DESTINATION_LEN, MAX_LINE, MAX_NAME_LEN, MAX_SESSION_ID_LEN, SamClient, SamClientState,
};
use i2pr_mail_sam::reply::{SamReplyKind, expect_destination, expect_naming_value, parse_reply};

#[test]
fn probe_injection_cannot_alter_command_structure() {
    // Values that would cross a token boundary if they were not rejected.
    let boundary_breaking = [
        "pop.postman.i2p EXTRA=1",
        "pop.postman.i2p\nSESSION CREATE",
        "a\"b",
        "a\\b",
        "a\nb",
        "a\tb",
        "a b",
        "\u{00}",
        "caf\u{e9}",
    ];
    for value in boundary_breaking {
        assert!(
            i2pr_mail_sam::client::naming_lookup_command(value).is_err(),
            "hostile name {value:?} was accepted"
        );
        assert!(
            i2pr_mail_sam::client::session_create_command(value, "dest").is_err(),
            "hostile id {value:?} was accepted"
        );
    }

    // `=` alone does NOT cross a boundary: SAM splits on whitespace first, then on
    // the first `=`, so `NAME==v` is one token NAME with value "=v". It is safe and
    // must stay accepted; what must never happen is a second token appearing.
    let line = i2pr_mail_sam::client::naming_lookup_command("=value").expect("= is not a break");
    assert_eq!(line, b"NAMING LOOKUP NAME==value\n");
    assert_eq!(String::from_utf8_lossy(&line).split_whitespace().count(), 3);

    // An over-long name is refused by the name bound, which is stricter than the
    // line ceiling and must fire first rather than letting the value reach the line.
    let overlong = "x".repeat(MAX_NAME_LEN + 1);
    assert!(i2pr_mail_sam::client::naming_lookup_command(&overlong).is_err());
    let overlong_id = "x".repeat(MAX_SESSION_ID_LEN + 1);
    assert!(i2pr_mail_sam::client::session_create_command(&overlong_id, "dest").is_err());
}

#[test]
fn probe_generated_lines_are_well_formed() {
    let line = i2pr_mail_sam::client::naming_lookup_command("pop.postman.i2p").expect("valid name");
    assert_eq!(line, b"NAMING LOOKUP NAME=pop.postman.i2p\n");

    let line = i2pr_mail_sam::client::stream_connect_command("s1", "abc-~=").expect("valid");
    assert_eq!(line, b"STREAM CONNECT ID=s1 DESTINATION=abc-~=\n");

    // No double spaces anywhere: `KEY= VALUE` would be a malformed option.
    for line in [
        i2pr_mail_sam::client::naming_lookup_command("a.b.i2p").unwrap(),
        i2pr_mail_sam::client::session_create_command("id", "dest").unwrap(),
        i2pr_mail_sam::client::stream_connect_command("id", "dest").unwrap(),
        SamClient::new().hello_command(),
    ] {
        let text = String::from_utf8(line).unwrap();
        assert!(!text.contains("  "), "double space in {text:?}");
        assert!(text.ends_with('\n'));
        assert_eq!(text.matches('\n').count(), 1);
    }
}

#[test]
fn probe_both_stream_shapes_normalize_identically() {
    let spec = parse_reply(b"SAM mail:1 OK\n").expect("spec shape");
    let impl_shape = parse_reply(b"STREAM STATUS RESULT=OK").expect("i2pr shape");
    assert_eq!(spec.kind(), SamReplyKind::StreamStatus);
    assert_eq!(impl_shape.kind(), SamReplyKind::StreamStatus);
    assert_eq!(spec.result(), impl_shape.result());
    assert_eq!(
        expect_destination(&spec).ok(),
        expect_destination(&impl_shape).ok()
    );

    // A bare `SAM OK` has no session:stream anchor and must be rejected.
    assert!(
        parse_reply(b"SAM OK").is_err(),
        "bare SAM OK must not parse"
    );
}

#[test]
fn probe_line_ceiling_is_exact_and_never_truncates() {
    // Request side: the value bounds (MAX_NAME_LEN = 256) are stricter than
    // MAX_LINE, so the name bound is what actually prevents an over-long command.
    // The line ceiling is the outer backstop and is enforced on the reply side.
    let longest = "a".repeat(MAX_NAME_LEN);
    let line = i2pr_mail_sam::client::naming_lookup_command(&longest).expect("fits");
    assert!(line.len() <= MAX_LINE);
    assert_eq!(line.len(), "NAMING LOOKUP NAME=".len() + MAX_NAME_LEN + 1);

    // Reply side: exactly MAX_LINE parses; one byte more is refused, never truncated.
    let mut reply = b"STREAM STATUS RESULT=OK MESSAGE=\"".to_vec();
    reply.extend(std::iter::repeat_n(b'x', MAX_LINE - reply.len() - 2));
    reply.extend_from_slice(b"\"\n");
    assert_eq!(reply.len(), MAX_LINE);
    let parsed = parse_reply(&reply).expect("line at the ceiling parses");
    assert_eq!(parsed.kind(), SamReplyKind::StreamStatus);

    // One byte past the ceiling must be refused, never truncated into a valid value.
    // Built fresh so it is a well-formed line that differs from `reply` only in
    // length -- otherwise it could be refused for a malformed-quote reason instead
    // of the ceiling, and the test would pass for the wrong cause.
    let mut over = b"STREAM STATUS RESULT=OK MESSAGE=\"".to_vec();
    over.extend(std::iter::repeat_n(b'x', MAX_LINE - over.len() - 1));
    over.extend_from_slice(b"\"\n");
    assert_eq!(over.len(), MAX_LINE + 1);
    assert!(
        parse_reply(&over).is_err(),
        "over-ceiling reply must be refused"
    );

    // A truncated ceiling line must not yield a silently-short option value.
    let truncated = &reply[..reply.len() - 5];
    assert!(
        parse_reply(truncated).is_err(),
        "unterminated quote must be refused"
    );
}

#[test]
fn probe_unknown_results_and_shapes_fail_closed() {
    // An unrecognised RESULT token and an unrecognised reply shape are refused at
    // parse time; an unknown result is never coerced to Ok.
    for line in [
        &b"HELLO REPLY RESULT=TOTALLY_MADE_UP\n"[..],
        &b"SOMETHING ELSE RESULT=OK\n"[..],
        &b"STREAM STATUS\n"[..],
        &b"SAM mail OK\n"[..],
        &b"\n"[..],
    ] {
        assert!(
            parse_reply(line).is_err(),
            "{:?} must fail closed",
            String::from_utf8_lossy(line)
        );
    }
}

#[test]
fn probe_typed_reads_pull_the_destination() {
    let ok = parse_reply(b"NAMING REPLY RESULT=OK NAME=pop.postman.i2p VALUE=abc-~\n").unwrap();
    assert_eq!(expect_naming_value(&ok).expect("value"), "abc-~");

    let missing = parse_reply(b"NAMING REPLY RESULT=KEY_NOT_FOUND NAME=pop.postman.i2p\n").unwrap();
    assert!(expect_naming_value(&missing).is_err());
}

#[test]
fn probe_state_machine_rejects_out_of_order_use() {
    let mut client = SamClient::new();
    assert_eq!(client.state(), SamClientState::Unestablished);

    let hello = parse_reply(b"HELLO REPLY RESULT=OK VERSION=3.1\n").unwrap();
    client.handle_reply(&hello).expect("hello accepted");
    assert_eq!(client.state(), SamClientState::HelloEstablished);

    let session = parse_reply(b"SESSION STATUS RESULT=OK DESTINATION=abc\n").unwrap();
    client.handle_reply(&session).expect("session accepted");
    assert_eq!(client.state(), SamClientState::SessionEstablished);

    let stream = parse_reply(b"SAM mail:1 OK\n").unwrap();
    client.handle_reply(&stream).expect("stream accepted");

    // A fresh client may not skip the handshake.
    let mut fresh = SamClient::new();
    assert!(fresh.handle_reply(&session).is_err());
    assert!(fresh.handle_reply(&stream).is_err());
}

#[test]
fn probe_destination_alphabet_and_bounds() {
    assert_eq!(MAX_DESTINATION_LEN, 1024);
    assert_eq!(MAX_SESSION_ID_LEN, 256);
    let ok = format!("{}+", "a".repeat(516));
    assert!(
        i2pr_mail_sam::client::validate_destination(&ok).is_err(),
        "standard base64 rejected"
    );
    let i2p = "-".repeat(516);
    assert!(i2pr_mail_sam::client::validate_destination(&i2p).is_ok());
    assert!(
        i2pr_mail_sam::client::validate_destination("=").is_err(),
        "padding-only rejected"
    );
    assert!(i2pr_mail_sam::client::validate_destination(&"a".repeat(1025)).is_err());
}
