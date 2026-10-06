//! Reply parsing: tokenizing, bounds enforcement, and typed reads.
//!
//! Every reply enters through [`parse_reply`]. Nothing in this module trusts
//! the remote side: the line length, the ASCII-only body, the token count, the
//! option count and the result vocabulary are all checked before a value is
//! handed to a caller, and an unrecognized `RESULT=` is a typed error rather
//! than a silent default to [`SamResult::Ok`].

use crate::client::{MAX_LINE, MAX_OPTION_COUNT, MAX_TOKEN_COUNT};
use crate::version::{SamVersion, SamVersionError, parse_version};

/// The documented SAM result vocabulary.
///
/// An unrecognized result is rejected rather than coerced, because guessing
/// `Ok` for a result this codec does not know would let a router-reported
/// failure drive the client down a success path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamResult {
    Ok,
    NoVersion,
    InvalidKey,
    KeyNotFound,
    DuplicatedId,
    DuplicatedDestination,
    InvalidId,
    InvalidName,
    Timeout,
    CantReachPeer,
    ConnectionRefused,
    NotImplemented,
    I2pError,
}

impl SamResult {
    /// Returns the canonical wire spelling of this result.
    pub const fn as_str(self) -> &'static str {
        match self {
            SamResult::Ok => "OK",
            SamResult::NoVersion => "NOVERSION",
            SamResult::InvalidKey => "INVALID_KEY",
            SamResult::KeyNotFound => "KEY_NOT_FOUND",
            SamResult::DuplicatedId => "DUPLICATED_ID",
            SamResult::DuplicatedDestination => "DUPLICATED_DESTINATION",
            SamResult::InvalidId => "INVALID_ID",
            SamResult::InvalidName => "INVALID_NAME",
            SamResult::Timeout => "TIMEOUT",
            SamResult::CantReachPeer => "CANT_REACH_PEER",
            SamResult::ConnectionRefused => "CONNECTION_REFUSED",
            SamResult::NotImplemented => "NOT_IMPLEMENTED",
            SamResult::I2pError => "I2P_ERROR",
        }
    }

    /// Parses a wire result token, failing closed on anything unknown.
    pub fn parse(input: &str) -> Result<SamResult, SamReplyError> {
        match input {
            "OK" => Ok(SamResult::Ok),
            "NOVERSION" => Ok(SamResult::NoVersion),
            "INVALID_KEY" => Ok(SamResult::InvalidKey),
            "KEY_NOT_FOUND" => Ok(SamResult::KeyNotFound),
            "DUPLICATED_ID" => Ok(SamResult::DuplicatedId),
            "DUPLICATED_DESTINATION" => Ok(SamResult::DuplicatedDestination),
            "INVALID_ID" => Ok(SamResult::InvalidId),
            "INVALID_NAME" => Ok(SamResult::InvalidName),
            "TIMEOUT" => Ok(SamResult::Timeout),
            "CANT_REACH_PEER" => Ok(SamResult::CantReachPeer),
            "CONNECTION_REFUSED" => Ok(SamResult::ConnectionRefused),
            "NOT_IMPLEMENTED" => Ok(SamResult::NotImplemented),
            "I2P_ERROR" => Ok(SamResult::I2pError),
            _ => Err(SamReplyError::UnknownResult),
        }
    }
}

/// Which reply a line is, independent of how it was spelled on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamReplyKind {
    HelloReply,
    NamingReply,
    SessionStatus,
    /// Both the specification spelling `SAM <session>:<stream> OK` and the
    /// router spelling `STREAM STATUS RESULT=OK` normalize to this kind.
    StreamStatus,
    Pong,
}

impl SamReplyKind {
    /// Returns the canonical wire spelling of this reply kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            SamReplyKind::HelloReply => "HELLO REPLY",
            SamReplyKind::NamingReply => "NAMING REPLY",
            SamReplyKind::SessionStatus => "SESSION STATUS",
            SamReplyKind::StreamStatus => "STREAM STATUS",
            SamReplyKind::Pong => "PONG",
        }
    }
}

/// A parsed, bounded SAM reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamReply {
    kind: SamReplyKind,
    result: SamResult,
    options: Vec<(String, String)>,
}

impl SamReply {
    /// Returns the reply kind.
    pub const fn kind(&self) -> SamReplyKind {
        self.kind
    }

    /// Returns the result code.
    pub const fn result(&self) -> SamResult {
        self.result
    }

    /// Returns the first value for `key`, if the reply carried it.
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Returns every parsed option in wire order. Unrecognized options are
    /// retained rather than dropped so an adapter can inspect them without
    /// widening the typed surface for each router extension.
    pub fn options(&self) -> &[(String, String)] {
        &self.options
    }
}

/// Why a reply line could not be parsed or read.
///
/// No variant carries the offending value, so a diagnostic built from this
/// error cannot leak a destination, a name, or a message body fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SamReplyError {
    /// The line exceeded [`MAX_LINE`]. A longer line is rejected outright and
    /// never truncated, because truncation would yield a plausible-looking
    /// reply with different content than the router sent.
    LineTooLong,
    /// The body was not printable ASCII. Tab, DEL, control bytes and
    /// multi-byte UTF-8 are all refused: SAM lines are ASCII by definition and
    /// a non-ASCII body suggests the transport is not delivering SAM at all.
    NotAscii,
    /// The line was empty, or was only a line terminator.
    Empty,
    /// The line carried more than [`MAX_TOKEN_COUNT`] tokens.
    TooManyTokens,
    /// The line carried more than [`MAX_OPTION_COUNT`] `KEY=VALUE` options.
    TooManyOptions,
    /// The `RESULT=` value is not in the documented vocabulary.
    UnknownResult,
    /// The reply shape requires a result and none was present.
    MissingResult,
    /// A double quote was opened and never closed.
    UnterminatedQuote,
    /// A non-`SAM` token in option position carried no `=`, or had an empty key.
    MalformedOption,
    /// The leading tokens do not name a reply this codec recognizes.
    UnexpectedShape,
    /// A typed read required `RESULT=OK` and found another result.
    UnexpectedResult(SamResult),
    /// A typed read required an option that was absent.
    MissingOption(&'static str),
    /// A `VERSION=` value did not parse as a version.
    InvalidVersion(SamVersionError),
}

/// Splits a reply body into tokens, honoring double-quoted values.
///
/// Inside quotes, `\` escapes the next byte and a literal `"` is produced. An
/// unterminated quote is an error rather than a value extending to end of
/// line, so a truncated reply cannot be read as a complete one.
fn split_tokens(body: &[u8]) -> Result<Vec<String>, SamReplyError> {
    let mut tokens = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut in_quotes = false;
    let mut quoted = false;
    let mut index = 0usize;
    while index < body.len() {
        let byte = body[index];
        if in_quotes && byte == b'\\' {
            let escaped = *body
                .get(index + 1)
                .ok_or(SamReplyError::UnterminatedQuote)?;
            current.push(escaped);
            quoted = true;
            index += 2;
            continue;
        }
        if byte == b'"' {
            in_quotes = !in_quotes;
            quoted = true;
            index += 1;
            continue;
        }
        if byte == b' ' && !in_quotes {
            // Separator. Runs of spaces and leading spaces produce no tokens.
            if quoted || !current.is_empty() {
                tokens.push(token_to_string(&current)?);
                current.clear();
                quoted = false;
            }
            index += 1;
            continue;
        }
        current.push(byte);
        index += 1;
    }
    if in_quotes {
        return Err(SamReplyError::UnterminatedQuote);
    }
    if quoted || !current.is_empty() {
        tokens.push(token_to_string(&current)?);
    }
    Ok(tokens)
}

fn token_to_string(bytes: &[u8]) -> Result<String, SamReplyError> {
    // The body is already verified ASCII, so this cannot fail; it is mapped
    // rather than unwrapped to keep the crate free of panicking paths.
    String::from_utf8(bytes.to_vec()).map_err(|_| SamReplyError::NotAscii)
}

/// Parses one complete reply line, with or without a trailing newline.
///
/// A single trailing `\n`, and a `\r` before it, are stripped. Both documented
/// shapes of the stream-connect success reply are accepted and normalized to
/// [`SamReplyKind::StreamStatus`]: the SAM v3 specification documents
/// `SAM <session>:<stream> OK`, while the router this client will connect to
/// emits `STREAM STATUS RESULT=OK`. Accepting only the specification spelling
/// would fail every connection against a real router.
pub fn parse_reply(line: &[u8]) -> Result<SamReply, SamReplyError> {
    if line.len() > MAX_LINE {
        return Err(SamReplyError::LineTooLong);
    }
    let body = line.strip_suffix(b"\n").unwrap_or(line);
    let body = body.strip_suffix(b"\r").unwrap_or(body);
    if body.is_empty() {
        return Err(SamReplyError::Empty);
    }
    if !body.iter().all(|b| (0x20..=0x7e).contains(b)) {
        return Err(SamReplyError::NotAscii);
    }
    let tokens = split_tokens(body)?;
    if tokens.is_empty() {
        return Err(SamReplyError::Empty);
    }
    if tokens.len() > MAX_TOKEN_COUNT {
        return Err(SamReplyError::TooManyTokens);
    }

    // (kind, inline result for the `SAM` shape, index of the first option)
    let (kind, inline, option_start) = classify(&tokens)?;
    let option_tokens = tokens
        .get(option_start..)
        .ok_or(SamReplyError::UnexpectedShape)?;
    if option_tokens.len() > MAX_OPTION_COUNT {
        return Err(SamReplyError::TooManyOptions);
    }

    let mut result = match inline {
        Some(token) => Some(SamResult::parse(token)?),
        // SAM v3 defines PING/PONG with no result token and exactly one defined
        // outcome, so PONG normalizes to Ok instead of inventing a code.
        None if kind == SamReplyKind::Pong => Some(SamResult::Ok),
        None => None,
    };
    let mut options: Vec<(String, String)> = Vec::new();
    for token in option_tokens {
        let (key, value) = token
            .split_once('=')
            .ok_or(SamReplyError::MalformedOption)?;
        if key.is_empty() {
            return Err(SamReplyError::MalformedOption);
        }
        if key == "RESULT" && result.is_none() {
            result = Some(SamResult::parse(value)?);
            continue;
        }
        options.push((key.to_string(), value.to_string()));
    }
    let result = result.ok_or(SamReplyError::MissingResult)?;

    Ok(SamReply {
        kind,
        result,
        options,
    })
}

/// Identifies the reply kind and where its options begin.
fn classify(tokens: &[String]) -> Result<(SamReplyKind, Option<&str>, usize), SamReplyError> {
    let second_is = |expected: &str| tokens.get(1).is_some_and(|t| t == expected);
    match tokens[0].as_str() {
        "HELLO" if second_is("REPLY") => Ok((SamReplyKind::HelloReply, None, 2)),
        "NAMING" if second_is("REPLY") => Ok((SamReplyKind::NamingReply, None, 2)),
        "SESSION" if second_is("STATUS") => Ok((SamReplyKind::SessionStatus, None, 2)),
        "STREAM" if second_is("STATUS") => Ok((SamReplyKind::StreamStatus, None, 2)),
        "PONG" => Ok((SamReplyKind::Pong, None, 1)),
        // The specification spelling of a stream result: `SAM <session>:<stream>
        // <RESULT> [MESSAGE="..."]`. Both halves of the anchor must be
        // non-empty, so an unrelated line beginning with `SAM` is not mistaken
        // for a stream result.
        "SAM" => {
            let anchor = tokens.get(1).ok_or(SamReplyError::UnexpectedShape)?;
            let Some((session, stream)) = anchor.split_once(':') else {
                return Err(SamReplyError::UnexpectedShape);
            };
            if session.is_empty() || stream.is_empty() {
                return Err(SamReplyError::UnexpectedShape);
            }
            let result = tokens.get(2).ok_or(SamReplyError::UnexpectedShape)?;
            Ok((SamReplyKind::StreamStatus, Some(result.as_str()), 3))
        }
        _ => Err(SamReplyError::UnexpectedShape),
    }
}

/// Reads the negotiated version from a successful `HELLO REPLY`.
pub fn expect_hello(reply: &SamReply) -> Result<SamVersion, SamReplyError> {
    if reply.kind() != SamReplyKind::HelloReply {
        return Err(SamReplyError::UnexpectedShape);
    }
    if reply.result() != SamResult::Ok {
        return Err(SamReplyError::UnexpectedResult(reply.result()));
    }
    let raw = reply
        .option("VERSION")
        .ok_or(SamReplyError::MissingOption("VERSION"))?;
    parse_version(raw).map_err(SamReplyError::InvalidVersion)
}

/// Reads the resolved destination from a successful `NAMING REPLY`.
pub fn expect_naming_value(reply: &SamReply) -> Result<&str, SamReplyError> {
    if reply.kind() != SamReplyKind::NamingReply {
        return Err(SamReplyError::UnexpectedShape);
    }
    if reply.result() != SamResult::Ok {
        return Err(SamReplyError::UnexpectedResult(reply.result()));
    }
    reply
        .option("VALUE")
        .ok_or(SamReplyError::MissingOption("VALUE"))
}

/// Reads the assigned destination from a successful `SESSION STATUS`.
pub fn expect_destination(reply: &SamReply) -> Result<&str, SamReplyError> {
    if reply.kind() != SamReplyKind::SessionStatus {
        return Err(SamReplyError::UnexpectedShape);
    }
    if reply.result() != SamResult::Ok {
        return Err(SamReplyError::UnexpectedResult(reply.result()));
    }
    reply
        .option("DESTINATION")
        .ok_or(SamReplyError::MissingOption("DESTINATION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> Result<SamReply, SamReplyError> {
        parse_reply(line)
    }

    #[test]
    fn result_wire_spellings_round_trip() {
        let all = [
            SamResult::Ok,
            SamResult::NoVersion,
            SamResult::InvalidKey,
            SamResult::KeyNotFound,
            SamResult::DuplicatedId,
            SamResult::DuplicatedDestination,
            SamResult::InvalidId,
            SamResult::InvalidName,
            SamResult::Timeout,
            SamResult::CantReachPeer,
            SamResult::ConnectionRefused,
            SamResult::NotImplemented,
            SamResult::I2pError,
        ];
        for result in all {
            assert_eq!(SamResult::parse(result.as_str()), Ok(result));
        }
        assert_eq!(SamResult::Ok.as_str(), "OK");
        assert_eq!(SamResult::NoVersion.as_str(), "NOVERSION");
        assert_eq!(SamResult::I2pError.as_str(), "I2P_ERROR");
    }

    #[test]
    fn unknown_result_fails_closed_instead_of_defaulting_to_ok() {
        assert_eq!(SamResult::parse("MAYBE"), Err(SamReplyError::UnknownResult));
        assert_eq!(SamResult::parse(""), Err(SamReplyError::UnknownResult));
        assert_eq!(SamResult::parse("ok"), Err(SamReplyError::UnknownResult));
        assert_eq!(
            parse(b"HELLO REPLY RESULT=WAT VERSION=3.1"),
            Err(SamReplyError::UnknownResult)
        );
        // A failure result must not be readable as a success either.
        assert_eq!(
            parse(b"NAMING REPLY RESULT=KEY_NOT_FOUND").map(|r| r.result()),
            Ok(SamResult::KeyNotFound)
        );
    }

    #[test]
    fn parses_hello_reply_with_and_without_newline() {
        for raw in [
            b"HELLO REPLY RESULT=OK VERSION=3.1\n".as_slice(),
            b"HELLO REPLY RESULT=OK VERSION=3.1".as_slice(),
            b"HELLO REPLY RESULT=OK VERSION=3.1\r\n".as_slice(),
        ] {
            let reply = parse(raw).expect("hello reply");
            assert_eq!(reply.kind(), SamReplyKind::HelloReply);
            assert_eq!(reply.result(), SamResult::Ok);
            assert_eq!(reply.option("VERSION"), Some("3.1"));
            assert_eq!(expect_hello(&reply), Ok(SamVersion::new(3, 1)));
        }
    }

    #[test]
    fn parses_hello_failure_without_a_version() {
        let reply = parse(b"HELLO REPLY RESULT=NOVERSION MESSAGE=\"no compatible version\"\n")
            .expect("hello reply");
        assert_eq!(reply.result(), SamResult::NoVersion);
        assert_eq!(reply.option("MESSAGE"), Some("no compatible version"));
        assert_eq!(
            expect_hello(&reply),
            Err(SamReplyError::UnexpectedResult(SamResult::NoVersion))
        );
    }

    #[test]
    fn parses_naming_reply_and_session_status() {
        let naming = parse(b"NAMING REPLY RESULT=OK NAME=pop.postman.i2p VALUE=~abc123--\n")
            .expect("naming reply");
        assert_eq!(naming.kind(), SamReplyKind::NamingReply);
        assert_eq!(naming.option("NAME"), Some("pop.postman.i2p"));
        assert_eq!(expect_naming_value(&naming), Ok("~abc123--"));

        let session =
            parse(b"SESSION STATUS RESULT=OK DESTINATION=~sessionkey\n").expect("session status");
        assert_eq!(session.kind(), SamReplyKind::SessionStatus);
        assert_eq!(expect_destination(&session), Ok("~sessionkey"));
    }

    #[test]
    fn both_stream_connect_shapes_normalize_to_the_same_reply() {
        // Shape documented by the SAM v3 specification.
        let spec = parse(b"SAM mail:1 OK\n").expect("spec shape");
        // Shape emitted by the router implementation this client will use.
        let router = parse(b"STREAM STATUS RESULT=OK\n").expect("router shape");
        assert_eq!(spec.kind(), SamReplyKind::StreamStatus);
        assert_eq!(router.kind(), SamReplyKind::StreamStatus);
        assert_eq!(spec.kind(), router.kind());
        assert_eq!(spec.result(), SamResult::Ok);
        assert_eq!(router.result(), SamResult::Ok);

        // Errors in the specification spelling normalize too.
        let spec_error =
            parse(b"SAM mail:1 I2P_ERROR MESSAGE=\"no tunnels\"\n").expect("spec error");
        assert_eq!(spec_error.kind(), SamReplyKind::StreamStatus);
        assert_eq!(spec_error.result(), SamResult::I2pError);
    }

    #[test]
    fn sam_shape_requires_a_session_stream_anchor() {
        assert_eq!(parse(b"SAM OK"), Err(SamReplyError::UnexpectedShape));
        assert_eq!(parse(b"SAM mail OK"), Err(SamReplyError::UnexpectedShape));
        assert_eq!(parse(b"SAM :1 OK"), Err(SamReplyError::UnexpectedShape));
        assert_eq!(parse(b"SAM mail:1"), Err(SamReplyError::UnexpectedShape));
    }

    #[test]
    fn pong_has_no_result_token_and_normalizes_to_ok() {
        let reply = parse(b"PONG\n").expect("pong");
        assert_eq!(reply.kind(), SamReplyKind::Pong);
        assert_eq!(reply.result(), SamResult::Ok);
        assert!(reply.options().is_empty());
    }

    #[test]
    fn quoted_values_escaped_quotes_and_embedded_equals_all_parse() {
        let reply = parse(
            b"HELLO REPLY RESULT=OK MESSAGE=\"say \\\"hi\\\" now\" TOKEN=a=b=c SPACED=\"two words\"\n",
        )
        .expect("quoted reply");
        assert_eq!(reply.option("MESSAGE"), Some(r#"say "hi" now"#));
        assert_eq!(reply.option("TOKEN"), Some("a=b=c"));
        assert_eq!(reply.option("SPACED"), Some("two words"));
        // A quoted value with a space is one token, not several.
        assert_eq!(reply.options().len(), 3);
    }

    #[test]
    fn an_empty_quoted_value_is_a_present_empty_option() {
        let reply = parse(b"HELLO REPLY RESULT=OK MESSAGE=\"\"\n").expect("empty value");
        assert_eq!(reply.option("MESSAGE"), Some(""));
    }

    #[test]
    fn rejects_unterminated_quotes_and_dangling_escapes() {
        assert_eq!(
            parse(b"HELLO REPLY RESULT=OK MESSAGE=\"unterminated"),
            Err(SamReplyError::UnterminatedQuote)
        );
        assert_eq!(
            parse(b"HELLO REPLY RESULT=OK MESSAGE=\"trailing\\"),
            Err(SamReplyError::UnterminatedQuote)
        );
    }

    #[test]
    fn rejects_empty_and_non_ascii_lines() {
        assert_eq!(parse(b""), Err(SamReplyError::Empty));
        assert_eq!(parse(b"\n"), Err(SamReplyError::Empty));
        assert_eq!(parse(b"\r\n"), Err(SamReplyError::Empty));
        assert_eq!(parse(b"   "), Err(SamReplyError::Empty));
        assert_eq!(
            parse(b"HELLO REPLY RESULT=OK\tVERSION=3.1"),
            Err(SamReplyError::NotAscii)
        );
        assert_eq!(
            parse("HELLO REPLY RESULT=OK VERSION=3.\u{fc}1".as_bytes()),
            Err(SamReplyError::NotAscii)
        );
        assert_eq!(
            parse(b"HELLO REPLY RESULT=OK \x7f"),
            Err(SamReplyError::NotAscii)
        );
    }

    #[test]
    fn rejects_unknown_shapes_and_missing_results() {
        assert_eq!(
            parse(b"DEST REPLY RESULT=OK"),
            Err(SamReplyError::UnexpectedShape)
        );
        assert_eq!(
            parse(b"HELLO EXTRA RESULT=OK"),
            Err(SamReplyError::UnexpectedShape)
        );
        assert_eq!(
            parse(b"SESSION REPLY RESULT=OK"),
            Err(SamReplyError::UnexpectedShape)
        );
        assert_eq!(
            parse(b"STREAM REPLY RESULT=OK"),
            Err(SamReplyError::UnexpectedShape)
        );
        assert_eq!(
            parse(b"HELLO REPLY VERSION=3.1"),
            Err(SamReplyError::MissingResult)
        );
        assert_eq!(
            parse(b"SESSION STATUS DESTINATION=~x"),
            Err(SamReplyError::MissingResult)
        );
    }

    #[test]
    fn rejects_malformed_options() {
        assert_eq!(
            parse(b"SESSION STATUS RESULT=OK DESTINATION"),
            Err(SamReplyError::MalformedOption)
        );
        assert_eq!(
            parse(b"SESSION STATUS RESULT=OK =value"),
            Err(SamReplyError::MalformedOption)
        );
    }

    #[test]
    fn enforces_the_token_and_option_ceilings_without_truncating() {
        let padding = MAX_LINE - b"SESSION STATUS RESULT=OK PAD=\"\"\n".len();
        let at_limit = format!("SESSION STATUS RESULT=OK PAD=\"{}\"\n", "a".repeat(padding));
        assert_eq!(at_limit.len(), MAX_LINE);
        let reply = parse(at_limit.as_bytes()).expect("line at ceiling");
        assert_eq!(reply.result(), SamResult::Ok);
        assert_eq!(reply.option("PAD").map(str::len), Some(padding));

        let over_limit = format!("{at_limit}a");
        assert_eq!(over_limit.len(), MAX_LINE + 1);
        assert_eq!(
            parse(over_limit.as_bytes()),
            Err(SamReplyError::LineTooLong)
        );

        let too_many_options = format!(
            "SESSION STATUS RESULT=OK {}",
            (0..=MAX_OPTION_COUNT)
                .map(|i| format!("K{i}=v"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert_eq!(
            parse(too_many_options.as_bytes()),
            Err(SamReplyError::TooManyOptions)
        );

        let too_many_tokens = (0..=MAX_TOKEN_COUNT)
            .map(|i| format!("K{i}=v"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            parse(too_many_tokens.as_bytes()),
            Err(SamReplyError::TooManyTokens)
        );
    }

    #[test]
    fn typed_reads_reject_the_wrong_kind_or_a_missing_option() {
        let hello = parse(b"HELLO REPLY RESULT=OK VERSION=3.1").expect("hello");
        assert_eq!(
            expect_destination(&hello),
            Err(SamReplyError::UnexpectedShape)
        );
        assert_eq!(
            expect_naming_value(&hello),
            Err(SamReplyError::UnexpectedShape)
        );
        assert_eq!(expect_hello(&hello), Ok(SamVersion::new(3, 1)));

        let session = parse(b"SESSION STATUS RESULT=OK").expect("session");
        assert_eq!(
            expect_destination(&session),
            Err(SamReplyError::MissingOption("DESTINATION"))
        );
        let naming = parse(b"NAMING REPLY RESULT=OK").expect("naming");
        assert_eq!(
            expect_naming_value(&naming),
            Err(SamReplyError::MissingOption("VALUE"))
        );
        let bad_version = parse(b"HELLO REPLY RESULT=OK VERSION=3").expect("bad version");
        assert_eq!(
            expect_hello(&bad_version),
            Err(SamReplyError::InvalidVersion(SamVersionError::Malformed))
        );
    }

    #[test]
    fn reply_kind_and_result_canonical_spellings() {
        assert_eq!(SamReplyKind::HelloReply.as_str(), "HELLO REPLY");
        assert_eq!(SamReplyKind::NamingReply.as_str(), "NAMING REPLY");
        assert_eq!(SamReplyKind::SessionStatus.as_str(), "SESSION STATUS");
        assert_eq!(SamReplyKind::StreamStatus.as_str(), "STREAM STATUS");
        assert_eq!(SamReplyKind::Pong.as_str(), "PONG");
        let router = parse(b"STREAM STATUS RESULT=CONNECTION_REFUSED").expect("refused");
        assert_eq!(router.result().as_str(), "CONNECTION_REFUSED");
        assert_eq!(router.options().len(), 0);
    }

    #[test]
    fn unknown_options_are_retained_in_wire_order() {
        let reply = parse(b"HELLO REPLY RESULT=OK VERSION=3.1 ALPHA=1 BETA=2").expect("extra");
        // RESULT is lifted into the typed field, so the remaining options are
        // retained in wire order, VERSION first.
        assert_eq!(reply.options().len(), 3);
        assert_eq!(reply.options()[0].0, "VERSION");
        assert_eq!(reply.options()[1].0, "ALPHA");
        assert_eq!(reply.options()[2].0, "BETA");
        assert_eq!(reply.option("BETA"), Some("2"));
    }
}
