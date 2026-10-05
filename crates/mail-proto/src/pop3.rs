pub const MAX_LINE: usize = 8192;
/// RFC 1939 command-length ceiling for `USER`. Values longer than this are
/// rejected before a command buffer is formatted or written.
pub const MAX_USER_LEN: usize = 40;
/// RFC 1939 command-length ceiling for `PASS`. The value is a credential, so a
/// rejection never carries the offending secret.
pub const MAX_PASS_LEN: usize = 40;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pop3Reply {
    Status { positive: bool, text: String },
    Multiline(Vec<Vec<u8>>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pop3Error {
    LineTooLong,
    InvalidStatus,
    MissingTerminator,
    InvalidCredential,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteEntry {
    pub ordinal: u32,
    pub uidl: String,
    pub size: Option<u64>,
}

pub fn parse_uidl_listing(lines: &[Vec<u8>]) -> Result<Vec<RemoteEntry>, Pop3Error> {
    match parse_multiline(lines)? {
        Pop3Reply::Multiline(entries) => entries
            .into_iter()
            .map(|line| {
                let text = String::from_utf8(line).map_err(|_| Pop3Error::InvalidStatus)?;
                let mut fields = text.split_ascii_whitespace();
                let ordinal = fields
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|n| *n > 0)
                    .ok_or(Pop3Error::InvalidStatus)?;
                let uidl = fields
                    .next()
                    .filter(|s| !s.is_empty() && s.len() <= 1024)
                    .ok_or(Pop3Error::InvalidStatus)?;
                if fields.next().is_some() {
                    return Err(Pop3Error::InvalidStatus);
                }
                Ok(RemoteEntry {
                    ordinal,
                    uidl: uidl.to_owned(),
                    size: None,
                })
            })
            .collect(),
        _ => Err(Pop3Error::InvalidStatus),
    }
}

pub fn parse_list_listing(lines: &[Vec<u8>]) -> Result<Vec<(u32, u64)>, Pop3Error> {
    match parse_multiline(lines)? {
        Pop3Reply::Multiline(entries) => entries
            .into_iter()
            .map(|line| {
                let text = String::from_utf8(line).map_err(|_| Pop3Error::InvalidStatus)?;
                let mut fields = text.split_ascii_whitespace();
                let ordinal = fields
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|n| *n > 0)
                    .ok_or(Pop3Error::InvalidStatus)?;
                let size = fields
                    .next()
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or(Pop3Error::InvalidStatus)?;
                if fields.next().is_some() {
                    return Err(Pop3Error::InvalidStatus);
                }
                Ok((ordinal, size))
            })
            .collect(),
        _ => Err(Pop3Error::InvalidStatus),
    }
}

/// Recognizes exactly `+OK` or `-ERR` status atoms. The atom must be followed by
/// either end-of-line or a single space delimiter, so truncated or glued forms
/// such as `-ER`, `-ERRX`, and `+OKX` are rejected instead of being read as a
/// negative status with stray text.
pub fn parse_status(line: &[u8]) -> Result<Pop3Reply, Pop3Error> {
    if line.len() > MAX_LINE {
        return Err(Pop3Error::LineTooLong);
    }
    let line = line
        .strip_suffix(b"\r\n")
        .ok_or(Pop3Error::MissingTerminator)?;
    let (positive, text) = if let Some(text) = line.strip_prefix(b"+OK") {
        (true, text)
    } else if let Some(text) = line.strip_prefix(b"-ERR") {
        (false, text)
    } else {
        return Err(Pop3Error::InvalidStatus);
    };
    let text = match text {
        b"" => b"",
        text if text[0] == b' ' => &text[1..],
        _ => return Err(Pop3Error::InvalidStatus),
    };
    Ok(Pop3Reply::Status {
        positive,
        text: String::from_utf8_lossy(text).trim().to_owned(),
    })
}

/// Bounds a POP3 credential before any command buffer is formatted. A credential
/// is a protocol line atom, so control bytes and delimiters are rejected too.
/// The returned error carries no part of the credential.
fn check_credential(value: &str, max: usize) -> Result<(), Pop3Error> {
    if value.is_empty() || value.len() > max || !value.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        Err(Pop3Error::InvalidCredential)
    } else {
        Ok(())
    }
}

pub fn validate_user(username: &str) -> Result<(), Pop3Error> {
    check_credential(username, MAX_USER_LEN)
}

pub fn validate_pass(password: &str) -> Result<(), Pop3Error> {
    check_credential(password, MAX_PASS_LEN)
}

/// Formats one bounded command line, including CRLF.
fn finish_command(line: &mut Vec<u8>) -> Result<Vec<u8>, Pop3Error> {
    line.extend_from_slice(b"\r\n");
    if line.len() > MAX_LINE {
        return Err(Pop3Error::LineTooLong);
    }
    Ok(std::mem::take(line))
}

/// Builds the complete `USER` command line only when the credential is within the
/// frozen ceiling and a valid protocol line atom.
pub fn user_command(username: &str) -> Result<Vec<u8>, Pop3Error> {
    validate_user(username)?;
    let mut line = Vec::with_capacity(MAX_USER_LEN + 8);
    line.extend_from_slice(b"USER ");
    line.extend_from_slice(username.as_bytes());
    finish_command(&mut line)
}

/// Builds the complete `PASS` command line under the frozen ceiling. The buffer
/// is ephemeral and is never logged, retained, or included in an error.
pub fn pass_command(password: &str) -> Result<Vec<u8>, Pop3Error> {
    validate_pass(password)?;
    let mut line = Vec::with_capacity(MAX_PASS_LEN + 8);
    line.extend_from_slice(b"PASS ");
    line.extend_from_slice(password.as_bytes());
    finish_command(&mut line)
}

fn ordinal_command(verb: &[u8], ordinal: u32) -> Result<Vec<u8>, Pop3Error> {
    if ordinal == 0 {
        return Err(Pop3Error::InvalidStatus);
    }
    let mut line = Vec::with_capacity(verb.len() + 12);
    line.extend_from_slice(verb);
    line.push(b' ');
    line.extend_from_slice(ordinal.to_string().as_bytes());
    finish_command(&mut line)
}

/// Builds a bounded message-scoped command line such as `DELE 1\r\n`.
pub fn dele_command(ordinal: u32) -> Result<Vec<u8>, Pop3Error> {
    ordinal_command(b"DELE", ordinal)
}

/// Builds the bounded full-entity retrieval line `RETR <ordinal>`.
pub fn retr_command(ordinal: u32) -> Result<Vec<u8>, Pop3Error> {
    ordinal_command(b"RETR", ordinal)
}

/// Builds the bounded header fetch line `TOP <ordinal> 0`, which requests the
/// zero header lines after the message header.
pub fn top_command(ordinal: u32) -> Result<Vec<u8>, Pop3Error> {
    if ordinal == 0 {
        return Err(Pop3Error::InvalidStatus);
    }
    let mut line = Vec::with_capacity(20);
    line.extend_from_slice(b"TOP ");
    line.extend_from_slice(ordinal.to_string().as_bytes());
    line.extend_from_slice(b" 0");
    finish_command(&mut line)
}

pub fn parse_multiline(lines: &[Vec<u8>]) -> Result<Pop3Reply, Pop3Error> {
    let mut out = Vec::new();
    for line in lines {
        if line.len() > MAX_LINE {
            return Err(Pop3Error::LineTooLong);
        }
        if line == b".\r\n" || line == b"." {
            return Ok(Pop3Reply::Multiline(out));
        }
        if line.starts_with(b"..") {
            out.push(line[1..].to_vec());
        } else {
            out.push(line.clone());
        }
    }
    Err(Pop3Error::MissingTerminator)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_status_and_dot_stuffing() {
        assert!(matches!(
            parse_status(b"+OK ready\r\n"),
            Ok(Pop3Reply::Status { positive: true, .. })
        ));
        assert_eq!(
            parse_multiline(&[b"..dot\r\n".to_vec(), b".\r\n".to_vec()]),
            Ok(Pop3Reply::Multiline(vec![b".dot\r\n".to_vec()]))
        );
    }
    #[test]
    fn rejects_unbounded_or_unterminated_input() {
        assert_eq!(
            parse_status(&vec![b'x'; MAX_LINE + 1]),
            Err(Pop3Error::LineTooLong)
        );
        assert_eq!(
            parse_multiline(&[b"data".to_vec()]),
            Err(Pop3Error::MissingTerminator)
        );
    }

    #[test]
    fn uidl_values_are_opaque_and_ordinals_parse_per_snapshot() {
        let entries =
            parse_uidl_listing(&[b"1 ../opaque\r\n".to_vec(), b".\r\n".to_vec()]).unwrap();
        assert_eq!(entries[0].ordinal, 1);
        assert_eq!(entries[0].uidl, "../opaque");
        assert!(parse_uidl_listing(&[b"0 no\r\n".to_vec(), b".\r\n".to_vec()]).is_err());
    }

    #[test]
    fn status_atoms_are_recognized_exactly() {
        assert_eq!(
            parse_status(b"+OK\r\n"),
            Ok(Pop3Reply::Status {
                positive: true,
                text: String::new()
            })
        );
        assert_eq!(
            parse_status(b"+OK ready\r\n"),
            Ok(Pop3Reply::Status {
                positive: true,
                text: "ready".into()
            })
        );
        assert_eq!(
            parse_status(b"-ERR\r\n"),
            Ok(Pop3Reply::Status {
                positive: false,
                text: String::new()
            })
        );
        assert_eq!(
            parse_status(b"-ERR no such message\r\n"),
            Ok(Pop3Reply::Status {
                positive: false,
                text: "no such message".into()
            })
        );
        // A space-delimited status may carry further text verbatim.
        assert_eq!(
            parse_status(b"+OK  double  space\r\n"),
            Ok(Pop3Reply::Status {
                positive: true,
                text: "double  space".into()
            })
        );
    }

    #[test]
    fn truncated_or_glued_status_forms_are_rejected() {
        for line in [
            &b"-ER\r\n"[..],
            &b"-ERRX\r\n"[..],
            &b"-ERRS\r\n"[..],
            &b"+OKX\r\n"[..],
            &b"+Ok\r\n"[..],
            &b"OK\r\n"[..],
            &b"-\r\n"[..],
            &b"\r\n"[..],
        ] {
            assert_eq!(
                parse_status(line),
                Err(Pop3Error::InvalidStatus),
                "{line:?}"
            );
        }
        // Status lines must be CRLF terminated.
        assert_eq!(
            parse_status(b"+OK bare\n"),
            Err(Pop3Error::MissingTerminator)
        );
        assert_eq!(parse_status(b"-ERR"), Err(Pop3Error::MissingTerminator));
    }

    #[test]
    fn credential_ceilings_are_enforced_before_formatting() {
        assert_eq!(validate_user(&"a".repeat(MAX_USER_LEN)), Ok(()));
        assert_eq!(
            validate_user(&"a".repeat(MAX_USER_LEN + 1)),
            Err(Pop3Error::InvalidCredential)
        );
        assert_eq!(validate_pass(&"a".repeat(MAX_PASS_LEN)), Ok(()));
        assert_eq!(
            validate_pass(&"a".repeat(MAX_PASS_LEN + 1)),
            Err(Pop3Error::InvalidCredential)
        );
        for value in ["", "with space\tcontrol", "cr\r\nlf", "delimiter\r\n"] {
            assert_eq!(validate_user(value), Err(Pop3Error::InvalidCredential));
            assert_eq!(validate_pass(value), Err(Pop3Error::InvalidCredential));
        }
        assert_eq!(user_command("alice").unwrap(), b"USER alice\r\n");
        assert_eq!(pass_command("s3cr3t").unwrap(), b"PASS s3cr3t\r\n");
        assert!(user_command(&"a".repeat(MAX_USER_LEN + 1)).is_err());
        assert!(pass_command(&"a".repeat(MAX_PASS_LEN + 1)).is_err());
    }

    #[test]
    fn message_scoped_commands_are_bounded() {
        assert_eq!(dele_command(7).unwrap(), b"DELE 7\r\n");
        assert_eq!(retr_command(u32::MAX).unwrap(), b"RETR 4294967295\r\n");
        assert_eq!(top_command(3).unwrap(), b"TOP 3 0\r\n");
        for command in [dele_command(0), retr_command(0), top_command(0)] {
            assert_eq!(command, Err(Pop3Error::InvalidStatus));
        }
    }
}
