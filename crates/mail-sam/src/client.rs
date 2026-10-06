//! Command formatting and the client-side ordering state machine.
//!
//! This module owns the bounds that both directions depend on: the line
//! ceiling, the token and option ceilings, and the per-field length ceilings.
//! [`crate::reply`] reads the ceilings it needs from here so there is exactly
//! one authoritative definition of each number.
//!
//! Every builder validates before it formats. No builder can emit an
//! over-budget line, and none can emit a line whose value came from a caller
//! unchecked, so a name or destination supplied by untrusted input cannot add
//! a SAM token, split a line, or carry a byte the router will not accept.

use crate::reply::{
    SamReply, SamReplyError, SamReplyKind, SamResult, expect_destination, expect_hello,
};
use crate::version::{MAX_SUPPORTED, MIN_SUPPORTED, SamVersion};

/// Ceiling on any line this client generates or accepts.
///
/// A generated line is checked against this before it is returned, and a
/// received line longer than this is rejected rather than truncated, because a
/// truncated reply would silently differ from what the router sent.
pub const MAX_LINE: usize = 8192;
/// Ceiling on a SAM name, such as `pop.postman.i2p`. Long enough for any
/// hostname plus a subdomain, short enough that a name cannot become a payload.
pub const MAX_NAME_LEN: usize = 256;
/// Ceiling on a session id. Bounded for the same reason as [`MAX_NAME_LEN`].
pub const MAX_SESSION_ID_LEN: usize = 256;
/// Ceiling on a base64 destination or key. A real I2P destination is roughly
/// 516 characters and a private key 884, so this leaves headroom without
/// letting an unbounded blob reach the transport.
pub const MAX_DESTINATION_LEN: usize = 1024;
/// Ceiling on tokens in a reply line.
pub const MAX_TOKEN_COUNT: usize = 64;
/// Ceiling on `KEY=VALUE` options in a reply line.
pub const MAX_OPTION_COUNT: usize = 32;

/// Why a command line could not be built.
///
/// No variant carries the offending value, so an error surfaced to a caller
/// cannot echo a name or destination back into a log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SamCommandError {
    /// The formatted line exceeded [`MAX_LINE`]. Checked after formatting as a
    /// backstop behind the per-field ceilings.
    LineTooLong,
    /// A name was empty, over-length, or contained a byte that could break a
    /// token. See [`validate_name`].
    InvalidName,
    /// A session id was empty, over-length, or contained a byte that could
    /// break a token. See [`validate_session_id`].
    InvalidSessionId,
    /// A destination was empty, over-length, or outside the I2P base64
    /// alphabet. See [`validate_destination`].
    InvalidDestination,
}

/// Why the state machine refused a reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SamProtocolError {
    /// The reply arrived before the state it depends on. SAM requires a
    /// session before a stream, and a handshake before a session, so acting on
    /// an out-of-order reply would mean assuming router state this client has
    /// not confirmed.
    OutOfOrder {
        required: SamClientState,
        actual: SamClientState,
    },
    /// The reply kind does not drive a state transition here. `NAMING REPLY`
    /// and `PONG` are legal at any time and are read through
    /// [`crate::reply::expect_naming_value`] at the call site instead.
    UnexpectedReplyKind { kind: SamReplyKind },
    /// The reply kind was right but its result was not `OK`.
    UnexpectedResult {
        kind: SamReplyKind,
        result: SamResult,
    },
    /// The reply was structurally valid but did not carry what this transition
    /// requires, for example a `SESSION STATUS` with no `DESTINATION`.
    InvalidReply(SamReplyError),
}

/// How far the client-side handshake has progressed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SamClientState {
    /// No `HELLO REPLY` yet. This is the default: a client starts here.
    #[default]
    Unestablished,
    /// Version negotiated; sessions may be created.
    HelloEstablished,
    /// A session exists; streams may be connected.
    SessionEstablished,
}

/// A state transition the client has accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamEvent {
    /// The negotiated version recorded by a successful `HELLO REPLY`.
    HelloEstablished(SamVersion),
    /// A session exists for this connection.
    SessionEstablished,
    /// A stream connect succeeded, under either reply spelling.
    StreamConnected,
}

/// Client-side SAM handshake state.
///
/// Holds only what this client decided: the negotiated version and the
/// session id it chose. It performs no I/O of any kind; an adapter feeds it
/// replies and sends the lines its builders return.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SamClient {
    state: SamClientState,
    version: Option<SamVersion>,
    session_id: Option<String>,
}

impl SamClient {
    /// Creates a client that has not yet completed a handshake.
    pub fn new() -> SamClient {
        SamClient::default()
    }

    /// Returns the current handshake state.
    pub fn state(&self) -> SamClientState {
        self.state
    }

    /// Returns the negotiated version, once a `HELLO REPLY` has been accepted.
    pub fn version(&self) -> Option<SamVersion> {
        self.version
    }

    /// Returns the session id bound to this client, once one is set.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Records the client-chosen session id.
    ///
    /// The id is chosen locally and is not echoed by the router, so it is bound
    /// here once the `SESSION CREATE` line has been built. It is validated with
    /// the same rule as the command builder so the recorded value is always one
    /// that could legally have been sent.
    pub fn bind_session_id(&mut self, id: &str) -> Result<(), SamCommandError> {
        validate_session_id(id)?;
        self.session_id = Some(id.to_string());
        Ok(())
    }

    /// Builds `HELLO VERSION MIN=.. MAX=..`.
    ///
    /// Takes no arguments because the range is this build's own compiled
    /// constants, so the line is bounded by construction and cannot be
    /// influenced by caller input.
    pub fn hello_command(&self) -> Vec<u8> {
        let mut line = SamLine::new();
        line.token(b"HELLO");
        line.token(b"VERSION");
        line.pair("MIN", MIN_SUPPORTED.to_string().as_bytes());
        line.pair("MAX", MAX_SUPPORTED.to_string().as_bytes());
        line.into_bytes()
    }

    /// Folds one reply into the client state.
    ///
    /// Accepts the three transitions SAM defines an ordering for. Out-of-order
    /// use is refused: `SESSION CREATE` before a handshake, and `STREAM
    /// CONNECT` before a session, both yield
    /// [`SamProtocolError::OutOfOrder`] rather than an assumed state.
    pub fn handle_reply(&mut self, reply: &SamReply) -> Result<SamEvent, SamProtocolError> {
        match reply.kind() {
            SamReplyKind::HelloReply => {
                let version = expect_hello(reply).map_err(SamProtocolError::InvalidReply)?;
                self.version = Some(version);
                // A re-HELLO does not tear down an existing session, so the
                // state only advances.
                if self.state == SamClientState::Unestablished {
                    self.state = SamClientState::HelloEstablished;
                }
                Ok(SamEvent::HelloEstablished(version))
            }
            SamReplyKind::SessionStatus => {
                if self.state == SamClientState::Unestablished {
                    return Err(SamProtocolError::OutOfOrder {
                        required: SamClientState::HelloEstablished,
                        actual: SamClientState::Unestablished,
                    });
                }
                if reply.result() != SamResult::Ok {
                    return Err(SamProtocolError::UnexpectedResult {
                        kind: reply.kind(),
                        result: reply.result(),
                    });
                }
                // The assigned destination must be present before the session
                // is treated as usable.
                expect_destination(reply).map_err(SamProtocolError::InvalidReply)?;
                self.state = SamClientState::SessionEstablished;
                Ok(SamEvent::SessionEstablished)
            }
            SamReplyKind::StreamStatus => {
                if self.state != SamClientState::SessionEstablished {
                    return Err(SamProtocolError::OutOfOrder {
                        required: SamClientState::SessionEstablished,
                        actual: self.state,
                    });
                }
                if reply.result() != SamResult::Ok {
                    return Err(SamProtocolError::UnexpectedResult {
                        kind: reply.kind(),
                        result: reply.result(),
                    });
                }
                Ok(SamEvent::StreamConnected)
            }
            SamReplyKind::NamingReply | SamReplyKind::Pong => {
                Err(SamProtocolError::UnexpectedReplyKind { kind: reply.kind() })
            }
        }
    }
}

/// True when `value` is a SAM token body that cannot break out of its token.
///
/// Requires non-empty printable non-space ASCII and forbids `"` and `\`. Space
/// is the byte that separates tokens, so excluding it is what stops a
/// caller-supplied value from injecting an extra `KEY=VALUE` pair or splitting
/// a command into two lines. Quotes and backslashes are excluded so a value
/// cannot open a quoted region and absorb following tokens. Control bytes are
/// excluded so a value cannot terminate the line early. Non-ASCII is excluded
/// because SAM lines are ASCII; a multi-byte character would make the length
/// ceiling ambiguous.
///
/// `=` is deliberately allowed. Without a space, an `=` inside a value cannot
/// start a new token, so it is not a boundary break.
fn is_token_body(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b) && b != b'"' && b != b'\\')
}

/// Validates a SAM name against [`MAX_NAME_LEN`] and [`is_token_body`].
pub fn validate_name(name: &str) -> Result<(), SamCommandError> {
    if name.len() > MAX_NAME_LEN || !is_token_body(name) {
        Err(SamCommandError::InvalidName)
    } else {
        Ok(())
    }
}

/// Validates a session id against [`MAX_SESSION_ID_LEN`] and [`is_token_body`].
pub fn validate_session_id(id: &str) -> Result<(), SamCommandError> {
    if id.len() > MAX_SESSION_ID_LEN || !is_token_body(id) {
        Err(SamCommandError::InvalidSessionId)
    } else {
        Ok(())
    }
}

/// True when `value` uses only the I2P base64 alphabet.
///
/// The I2P alphabet is `A-Z`, `a-z`, `0-9`, `-` and `~`, plus `=` used solely
/// as a trailing pad. Standard base64's `+` and `/` are rejected here because
/// the router will not accept them, and forwarding one would turn a local
/// validation failure into a router-side session error. Whether the bytes
/// decode to a real destination is the router's business; this only
/// guarantees no byte outside the I2P alphabet is ever sent.
fn is_i2p_base64(value: &str) -> bool {
    let bytes = value.as_bytes();
    let body = bytes.iter().position(|b| *b == b'=').unwrap_or(bytes.len());
    // The body must be non-empty: padding alone is not a destination.
    body > 0
        && bytes[body..].iter().all(|b| *b == b'=')
        && bytes[..body].iter().all(|b| {
            b.is_ascii_uppercase()
                || b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || *b == b'-'
                || *b == b'~'
        })
}

/// Validates a destination against [`MAX_DESTINATION_LEN`] and the I2P base64
/// alphabet.
pub fn validate_destination(destination: &str) -> Result<(), SamCommandError> {
    if destination.is_empty()
        || destination.len() > MAX_DESTINATION_LEN
        || !is_i2p_base64(destination)
    {
        Err(SamCommandError::InvalidDestination)
    } else {
        Ok(())
    }
}

/// Accumulates one SAM command line.
///
/// Spacing is the caller's business through [`SamLine::token`] and
/// [`SamLine::pair`], and a `KEY=VALUE` option is produced by exactly one
/// [`SamLine::pair`] call. Formatting a line by joining fragments with a
/// separator invites a stray space between a key and its value, which the
/// router reads as a malformed option, so the builder makes that mistake
/// unrepresentable instead of relying on the caller to count spaces.
struct SamLine {
    bytes: Vec<u8>,
}

impl SamLine {
    fn new() -> SamLine {
        SamLine { bytes: Vec::new() }
    }

    /// Appends a bare keyword token, space-separated from what precedes it.
    fn token(&mut self, part: &[u8]) -> &mut SamLine {
        if !self.bytes.is_empty() {
            self.bytes.push(b' ');
        }
        self.bytes.extend_from_slice(part);
        self
    }

    /// Appends one `KEY=VALUE` option as a single space-separated unit.
    fn pair(&mut self, key: &str, value: &[u8]) -> &mut SamLine {
        self.bytes.push(b' ');
        self.bytes.extend_from_slice(key.as_bytes());
        self.bytes.push(b'=');
        self.bytes.extend_from_slice(value);
        self
    }

    /// Terminates the line and enforces the [`MAX_LINE`] ceiling.
    ///
    /// The check happens after formatting, so no builder can return an
    /// over-budget line even if a per-field ceiling were raised.
    fn finish(mut self) -> Result<Vec<u8>, SamCommandError> {
        self.bytes.push(b'\n');
        if self.bytes.len() > MAX_LINE {
            return Err(SamCommandError::LineTooLong);
        }
        Ok(self.bytes)
    }

    /// Terminates the line without a ceiling check.
    ///
    /// Only used where every part is a compile-time constant, so the result
    /// cannot exceed the line budget. The `debug_assert` makes that invariant
    /// visible and catches a future edit that breaks it.
    fn into_bytes(mut self) -> Vec<u8> {
        self.bytes.push(b'\n');
        debug_assert!(self.bytes.len() <= MAX_LINE);
        self.bytes
    }
}

/// Builds `NAMING LOOKUP NAME=<name>`.
///
/// Legal at any time, including before a session exists, because SAM routing
/// lookup does not depend on a session.
pub fn naming_lookup_command(name: &str) -> Result<Vec<u8>, SamCommandError> {
    validate_name(name)?;
    let mut line = SamLine::new();
    line.token(b"NAMING")
        .token(b"LOOKUP")
        .pair("NAME", name.as_bytes());
    line.finish()
}

/// Builds `SESSION CREATE STYLE=STREAM ID=<id> DESTINATION=<destination>`.
///
/// `STREAM` is the only style this client needs; a datagram or raw tunnel
/// session would not carry the POP3/SMTP byte stream this client is built for.
pub fn session_create_command(id: &str, destination: &str) -> Result<Vec<u8>, SamCommandError> {
    validate_session_id(id)?;
    validate_destination(destination)?;
    let mut line = SamLine::new();
    line.token(b"SESSION")
        .token(b"CREATE")
        .pair("STYLE", b"STREAM")
        .pair("ID", id.as_bytes())
        .pair("DESTINATION", destination.as_bytes());
    line.finish()
}

/// Builds `STREAM CONNECT ID=<id> DESTINATION=<destination>`.
pub fn stream_connect_command(id: &str, destination: &str) -> Result<Vec<u8>, SamCommandError> {
    validate_session_id(id)?;
    validate_destination(destination)?;
    let mut line = SamLine::new();
    line.token(b"STREAM")
        .token(b"CONNECT")
        .pair("ID", id.as_bytes())
        .pair("DESTINATION", destination.as_bytes());
    line.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reply::parse_reply;

    /// A destination in the real I2P alphabet, `-` and `~` heavy.
    fn destination(len: usize) -> String {
        "~".repeat(len)
    }

    #[test]
    fn hello_command_is_the_negotiated_range_including_newline() {
        let client = SamClient::new();
        assert_eq!(
            client.hello_command(),
            b"HELLO VERSION MIN=3.1 MAX=3.1\n".to_vec()
        );
        // Bounded by construction: it takes no caller input at all.
        assert!(client.hello_command().len() <= MAX_LINE);
        assert!(client.hello_command().ends_with(b"\n"));
    }

    #[test]
    fn command_builders_emit_exact_bytes_with_trailing_newline() {
        assert_eq!(
            naming_lookup_command("pop.postman.i2p").unwrap(),
            b"NAMING LOOKUP NAME=pop.postman.i2p\n".to_vec()
        );
        assert_eq!(
            session_create_command("mail", "~dest").unwrap(),
            b"SESSION CREATE STYLE=STREAM ID=mail DESTINATION=~dest\n".to_vec()
        );
        assert_eq!(
            stream_connect_command("mail", "~dest").unwrap(),
            b"STREAM CONNECT ID=mail DESTINATION=~dest\n".to_vec()
        );
    }

    #[test]
    fn builders_accept_the_largest_in_bounds_value() {
        let name = "a".repeat(MAX_NAME_LEN);
        assert_eq!(validate_name(&name), Ok(()));
        assert!(naming_lookup_command(&name).is_ok());

        let id = "s".repeat(MAX_SESSION_ID_LEN);
        assert_eq!(validate_session_id(&id), Ok(()));
        assert!(session_create_command(&id, "~d").is_ok());
        assert!(stream_connect_command(&id, "~d").is_ok());

        let dest = destination(MAX_DESTINATION_LEN);
        assert_eq!(validate_destination(&dest), Ok(()));
        assert!(session_create_command("mail", &dest).is_ok());

        assert_eq!(
            validate_name(&"a".repeat(MAX_NAME_LEN + 1)),
            Err(SamCommandError::InvalidName)
        );
        assert_eq!(
            validate_session_id(&"s".repeat(MAX_SESSION_ID_LEN + 1)),
            Err(SamCommandError::InvalidSessionId)
        );
        assert_eq!(
            validate_destination(&destination(MAX_DESTINATION_LEN + 1)),
            Err(SamCommandError::InvalidDestination)
        );
    }

    #[test]
    fn maximum_length_values_still_fit_the_line_ceiling() {
        let name_line = naming_lookup_command(&"a".repeat(MAX_NAME_LEN)).unwrap();
        let session_line = session_create_command(
            &"s".repeat(MAX_SESSION_ID_LEN),
            &destination(MAX_DESTINATION_LEN),
        )
        .unwrap();
        let stream_line = stream_connect_command(
            &"s".repeat(MAX_SESSION_ID_LEN),
            &destination(MAX_DESTINATION_LEN),
        )
        .unwrap();
        for line in [&name_line, &session_line, &stream_line] {
            assert!(line.len() <= MAX_LINE, "line of {} bytes", line.len());
            assert_eq!(line.iter().filter(|b| **b == b'\n').count(), 1);
            assert!(line.ends_with(b"\n"));
        }
    }

    #[test]
    fn builders_reject_empty_over_length_and_malformed_values() {
        assert_eq!(naming_lookup_command(""), Err(SamCommandError::InvalidName));
        assert_eq!(
            naming_lookup_command(&"a".repeat(MAX_NAME_LEN + 1)),
            Err(SamCommandError::InvalidName)
        );
        assert_eq!(
            session_create_command("", "~d"),
            Err(SamCommandError::InvalidSessionId)
        );
        assert_eq!(
            session_create_command(&"s".repeat(MAX_SESSION_ID_LEN + 1), "~d"),
            Err(SamCommandError::InvalidSessionId)
        );
        assert_eq!(
            stream_connect_command("", "~d"),
            Err(SamCommandError::InvalidSessionId)
        );
        assert_eq!(
            session_create_command("mail", ""),
            Err(SamCommandError::InvalidDestination)
        );
        assert_eq!(
            stream_connect_command("mail", ""),
            Err(SamCommandError::InvalidDestination)
        );
        assert_eq!(
            session_create_command("mail", &destination(MAX_DESTINATION_LEN + 1)),
            Err(SamCommandError::InvalidDestination)
        );
    }

    #[test]
    fn builders_reject_whitespace_quote_backslash_control_and_non_ascii() {
        let bad_values = [
            "with space",
            "tab\there",
            "cr\r\nlf",
            "quote\"inside",
            "back\\slash",
            "trailing ",
            " leading",
            "\u{0}",
            "caf\u{e9}",
            "\u{1f600}",
            "\u{fffd}",
        ];
        for value in bad_values {
            assert_eq!(
                validate_name(value),
                Err(SamCommandError::InvalidName),
                "name {value:?}"
            );
            assert_eq!(
                validate_session_id(value),
                Err(SamCommandError::InvalidSessionId),
                "id {value:?}"
            );
            assert_eq!(
                naming_lookup_command(value),
                Err(SamCommandError::InvalidName),
                "command {value:?}"
            );
            assert_eq!(
                session_create_command(value, "~d"),
                Err(SamCommandError::InvalidSessionId),
                "command {value:?}"
            );
        }
    }

    #[test]
    fn injection_attempts_cannot_add_a_token_or_split_a_line() {
        let attempts = [
            "mail RESULT=OK",
            "mail\nSTREAM CONNECT ID=x",
            "mail DESTINATION=~forged",
            "mail\" RESULT=OK NAME=evil",
            "mail\\ RESULT=OK",
            "x=1 y=2",
        ];
        for attempt in attempts {
            assert_eq!(
                stream_connect_command(attempt, "~d"),
                Err(SamCommandError::InvalidSessionId),
                "id {attempt:?}"
            );
            assert_eq!(
                naming_lookup_command(attempt),
                Err(SamCommandError::InvalidName),
                "name {attempt:?}"
            );
        }
    }

    #[test]
    fn a_surviving_injection_attempt_still_produces_one_command_line() {
        // `=` alone is not a boundary: without a space it cannot open a token.
        let line = session_create_command("mail=RESULT=OK", "~dest").unwrap();
        let text = String::from_utf8(line).unwrap();
        assert_eq!(
            text,
            "SESSION CREATE STYLE=STREAM ID=mail=RESULT=OK DESTINATION=~dest\n"
        );
        assert_eq!(text.lines().count(), 1);
        assert_eq!(text.matches("RESULT=").count(), 1);
        assert_eq!(text.matches("STYLE=").count(), 1);
        // The id survives intact as one option value.
        assert_eq!(
            text.split("ID=").nth(1).unwrap().split(' ').next(),
            Some("mail=RESULT=OK")
        );
    }

    #[test]
    fn destination_alphabet_accepts_i2p_base64_and_rejects_standard_base64() {
        let realistic: String = format!("{}{}", "~-".repeat(252), "abcdEFGH1234");
        assert_eq!(realistic.len(), 516);
        assert_eq!(validate_destination(&realistic), Ok(()));
        assert_eq!(validate_destination("~").map(|_| ()), Ok(()));
        assert_eq!(validate_destination("AAAA===="), Ok(()));
        for bad in [
            "abc+def",
            "abc/def",
            "with space",
            "tab\there",
            "new\nline",
            "quote\"",
            "back\\slash",
            "caf\u{e9}",
            "=",
            "AAAA=A",
        ] {
            assert_eq!(
                validate_destination(bad),
                Err(SamCommandError::InvalidDestination),
                "destination {bad:?}"
            );
        }
    }

    #[test]
    fn padded_and_unpadded_destinations_are_both_accepted() {
        assert_eq!(validate_destination("~k9w=="), Ok(()));
        assert_eq!(validate_destination("~k9w"), Ok(()));
        assert_eq!(validate_destination("~k9w="), Ok(()));
        // Padding must trail the body, never precede it.
        assert_eq!(
            validate_destination("=~k9w"),
            Err(SamCommandError::InvalidDestination)
        );
    }

    #[test]
    fn state_machine_advances_hello_then_session_then_stream() {
        let mut client = SamClient::new();
        assert_eq!(client.state(), SamClientState::Unestablished);

        let hello = parse_reply(b"HELLO REPLY RESULT=OK VERSION=3.1").unwrap();
        assert_eq!(
            client.handle_reply(&hello),
            Ok(SamEvent::HelloEstablished(SamVersion::new(3, 1)))
        );
        assert_eq!(client.state(), SamClientState::HelloEstablished);
        assert_eq!(client.version(), Some(SamVersion::new(3, 1)));

        assert_eq!(client.bind_session_id("mail"), Ok(()));
        assert_eq!(client.session_id(), Some("mail"));

        let session = parse_reply(b"SESSION STATUS RESULT=OK DESTINATION=~dest\n").unwrap();
        assert_eq!(
            client.handle_reply(&session),
            Ok(SamEvent::SessionEstablished)
        );
        assert_eq!(client.state(), SamClientState::SessionEstablished);

        // Both documented reply spellings drive the same transition.
        for raw in [
            b"STREAM STATUS RESULT=OK\n".as_slice(),
            b"SAM mail:1 OK\n".as_slice(),
        ] {
            let stream = parse_reply(raw).unwrap();
            assert_eq!(stream.kind(), SamReplyKind::StreamStatus);
            assert_eq!(client.handle_reply(&stream), Ok(SamEvent::StreamConnected));
            assert_eq!(client.state(), SamClientState::SessionEstablished);
        }
    }

    #[test]
    fn session_status_before_hello_is_out_of_order() {
        let mut client = SamClient::new();
        let reply = parse_reply(b"SESSION STATUS RESULT=OK DESTINATION=~dest").unwrap();
        assert_eq!(
            client.handle_reply(&reply),
            Err(SamProtocolError::OutOfOrder {
                required: SamClientState::HelloEstablished,
                actual: SamClientState::Unestablished,
            })
        );
        assert_eq!(client.state(), SamClientState::Unestablished);
    }

    #[test]
    fn stream_status_before_a_session_is_out_of_order() {
        let mut client = SamClient::new();
        let reply = parse_reply(b"STREAM STATUS RESULT=OK\n").unwrap();
        assert_eq!(
            client.handle_reply(&reply),
            Err(SamProtocolError::OutOfOrder {
                required: SamClientState::SessionEstablished,
                actual: SamClientState::Unestablished,
            })
        );

        let hello = parse_reply(b"HELLO REPLY RESULT=OK VERSION=3.1").unwrap();
        assert!(client.handle_reply(&hello).is_ok());
        assert_eq!(
            client.handle_reply(&reply),
            Err(SamProtocolError::OutOfOrder {
                required: SamClientState::SessionEstablished,
                actual: SamClientState::HelloEstablished,
            })
        );
        assert_eq!(client.state(), SamClientState::HelloEstablished);
    }

    #[test]
    fn a_failed_result_never_advances_state() {
        let mut client = SamClient::new();
        let no_version = parse_reply(b"HELLO REPLY RESULT=NOVERSION").unwrap();
        assert_eq!(
            client.handle_reply(&no_version),
            Err(SamProtocolError::InvalidReply(
                SamReplyError::UnexpectedResult(SamResult::NoVersion)
            ))
        );
        assert_eq!(client.state(), SamClientState::Unestablished);
        assert_eq!(client.version(), None);

        let hello = parse_reply(b"HELLO REPLY RESULT=OK VERSION=3.1").unwrap();
        assert!(client.handle_reply(&hello).is_ok());
        let dup = parse_reply(b"SESSION STATUS RESULT=DUPLICATED_ID").unwrap();
        assert_eq!(
            client.handle_reply(&dup),
            Err(SamProtocolError::UnexpectedResult {
                kind: SamReplyKind::SessionStatus,
                result: SamResult::DuplicatedId,
            })
        );
        assert_eq!(client.state(), SamClientState::HelloEstablished);

        let refused = parse_reply(b"STREAM STATUS RESULT=CONNECTION_REFUSED").unwrap();
        assert_eq!(
            client.handle_reply(&refused),
            Err(SamProtocolError::OutOfOrder {
                required: SamClientState::SessionEstablished,
                actual: SamClientState::HelloEstablished,
            })
        );
    }

    #[test]
    fn a_session_status_without_a_destination_is_rejected() {
        let mut client = SamClient::new();
        let hello = parse_reply(b"HELLO REPLY RESULT=OK VERSION=3.1").unwrap();
        assert!(client.handle_reply(&hello).is_ok());
        let reply = parse_reply(b"SESSION STATUS RESULT=OK").unwrap();
        assert_eq!(
            client.handle_reply(&reply),
            Err(SamProtocolError::InvalidReply(
                SamReplyError::MissingOption("DESTINATION")
            ))
        );
        assert_eq!(client.state(), SamClientState::HelloEstablished);
    }

    #[test]
    fn naming_and_pong_replies_are_not_state_transitions() {
        let mut client = SamClient::new();
        let naming =
            parse_reply(b"NAMING REPLY RESULT=OK NAME=pop.postman.i2p VALUE=~dest").unwrap();
        assert_eq!(
            client.handle_reply(&naming),
            Err(SamProtocolError::UnexpectedReplyKind {
                kind: SamReplyKind::NamingReply,
            })
        );
        // The same reply is still readable through the typed helper, which is
        // where a naming result belongs: it never changes session state.
        assert_eq!(crate::reply::expect_naming_value(&naming), Ok("~dest"));
        assert_eq!(client.state(), SamClientState::Unestablished);

        let pong = parse_reply(b"PONG").unwrap();
        assert_eq!(
            client.handle_reply(&pong),
            Err(SamProtocolError::UnexpectedReplyKind {
                kind: SamReplyKind::Pong,
            })
        );
    }

    #[test]
    fn bind_session_id_validates_before_recording() {
        let mut client = SamClient::new();
        assert_eq!(
            client.bind_session_id("bad id"),
            Err(SamCommandError::InvalidSessionId)
        );
        assert_eq!(client.session_id(), None);
        assert_eq!(client.bind_session_id("mail"), Ok(()));
        assert_eq!(client.session_id(), Some("mail"));
    }

    #[test]
    fn a_re_hello_does_not_tear_down_an_established_session() {
        let mut client = SamClient::new();
        let hello = parse_reply(b"HELLO REPLY RESULT=OK VERSION=3.1").unwrap();
        assert!(client.handle_reply(&hello).is_ok());
        let session = parse_reply(b"SESSION STATUS RESULT=OK DESTINATION=~d").unwrap();
        assert!(client.handle_reply(&session).is_ok());
        assert_eq!(
            client.handle_reply(&hello),
            Ok(SamEvent::HelloEstablished(SamVersion::new(3, 1)))
        );
        assert_eq!(client.state(), SamClientState::SessionEstablished);
    }

    #[test]
    fn full_deterministic_transcript_from_hello_to_stream() {
        let mut client = SamClient::new();
        let mut wire: Vec<String> = Vec::new();

        let hello = client.hello_command();
        wire.push(String::from_utf8(hello.clone()).unwrap());
        assert_eq!(
            hello,
            b"HELLO VERSION MIN=3.1 MAX=3.1\n".to_vec(),
            "bytes out"
        );

        let hello_reply = b"HELLO REPLY RESULT=OK VERSION=3.1\n";
        wire.push(String::from_utf8(hello_reply.to_vec()).unwrap());
        assert_eq!(
            client.handle_reply(&parse_reply(hello_reply).unwrap()),
            Ok(SamEvent::HelloEstablished(SamVersion::new(3, 1)))
        );

        let lookup = naming_lookup_command("pop.postman.i2p").unwrap();
        wire.push(String::from_utf8(lookup.clone()).unwrap());
        assert_eq!(lookup, b"NAMING LOOKUP NAME=pop.postman.i2p\n".to_vec());

        let lookup_reply = b"NAMING REPLY RESULT=OK NAME=pop.postman.i2p VALUE=~popdest--\n";
        wire.push(String::from_utf8(lookup_reply.to_vec()).unwrap());
        let resolved = crate::reply::expect_naming_value(&parse_reply(lookup_reply).unwrap())
            .unwrap()
            .to_string();

        let create = session_create_command("mail", &resolved).unwrap();
        wire.push(String::from_utf8(create.clone()).unwrap());
        assert_eq!(
            create,
            b"SESSION CREATE STYLE=STREAM ID=mail DESTINATION=~popdest--\n".to_vec()
        );
        assert_eq!(client.bind_session_id("mail"), Ok(()));

        let create_reply = b"SESSION STATUS RESULT=OK DESTINATION=~sessionkey\n";
        wire.push(String::from_utf8(create_reply.to_vec()).unwrap());
        assert_eq!(
            client.handle_reply(&parse_reply(create_reply).unwrap()),
            Ok(SamEvent::SessionEstablished)
        );

        let connect = stream_connect_command("mail", &resolved).unwrap();
        wire.push(String::from_utf8(connect.clone()).unwrap());
        assert_eq!(
            connect,
            b"STREAM CONNECT ID=mail DESTINATION=~popdest--\n".to_vec()
        );

        let connect_reply = b"STREAM STATUS RESULT=OK\n";
        wire.push(String::from_utf8(connect_reply.to_vec()).unwrap());
        assert_eq!(
            client.handle_reply(&parse_reply(connect_reply).unwrap()),
            Ok(SamEvent::StreamConnected)
        );
        assert_eq!(client.state(), SamClientState::SessionEstablished);

        assert_eq!(
            wire,
            vec![
                "HELLO VERSION MIN=3.1 MAX=3.1\n",
                "HELLO REPLY RESULT=OK VERSION=3.1\n",
                "NAMING LOOKUP NAME=pop.postman.i2p\n",
                "NAMING REPLY RESULT=OK NAME=pop.postman.i2p VALUE=~popdest--\n",
                "SESSION CREATE STYLE=STREAM ID=mail DESTINATION=~popdest--\n",
                "SESSION STATUS RESULT=OK DESTINATION=~sessionkey\n",
                "STREAM CONNECT ID=mail DESTINATION=~popdest--\n",
                "STREAM STATUS RESULT=OK\n",
            ]
        );
    }
}
