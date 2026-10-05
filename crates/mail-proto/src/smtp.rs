pub const MAX_LINE: usize = 8192;
/// Ceiling for one `AUTH LOGIN` challenge response line. AUTH LOGIN base64 expands
/// its input by 4/3, so this bounds the pre-expansion credential length.
pub const MAX_AUTH_LINE: usize = 1024;
/// Largest `MAIL FROM`/`RCPT TO` envelope line this client will generate,
/// including CRLF. An over-long address is rejected instead of being formatted.
pub const MAX_ENVELOPE_LINE: usize = 512;
/// Bound on a credential before base64 expansion, derived from `MAX_AUTH_LINE`.
pub const MAX_CREDENTIAL_LEN: usize = MAX_AUTH_LINE * 3 / 4;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmtpReply {
    pub code: u16,
    pub lines: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmtpError {
    LineTooLong,
    InvalidReply,
    InconsistentCode,
    MissingFinalLine,
    InvalidEnvelope,
    InvalidCredential,
}

/// Bounds a credential before base64 expansion. Errors never carry the secret.
fn check_credential(value: &str) -> Result<(), SmtpError> {
    if value.is_empty()
        || value.len() > MAX_CREDENTIAL_LEN
        || !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
    {
        Err(SmtpError::InvalidCredential)
    } else {
        Ok(())
    }
}

pub fn validate_username(username: &str) -> Result<(), SmtpError> {
    check_credential(username)
}

pub fn validate_password(password: &str) -> Result<(), SmtpError> {
    check_credential(password)
}

/// Expanded base64 line length for a credential of `len` bytes, used to reject an
/// over-budget AUTH LOGIN response before any encoding is performed.
pub fn expanded_auth_line_len(len: usize) -> usize {
    len.div_ceil(3) * 4
}

/// Builds a bounded envelope command line for an already-approved command verb.
/// The complete line must fit the frozen envelope budget.
fn envelope_command(verb: &[u8], address: &str) -> Result<Vec<u8>, SmtpError> {
    // The builder supplies the angle brackets, so an address carrying its own
    // would produce a malformed or ambiguous envelope line.
    if address.is_empty()
        || !address
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b) && b != b'<' && b != b'>')
    {
        return Err(SmtpError::InvalidEnvelope);
    }
    let mut line = Vec::with_capacity(MAX_ENVELOPE_LINE);
    line.extend_from_slice(verb);
    line.push(b'<');
    line.extend_from_slice(address.as_bytes());
    line.extend_from_slice(b">\r\n");
    if line.len() > MAX_ENVELOPE_LINE || line.len() > MAX_LINE {
        return Err(SmtpError::InvalidEnvelope);
    }
    Ok(line)
}

pub fn mail_from_command(from: &str) -> Result<Vec<u8>, SmtpError> {
    envelope_command(b"MAIL FROM:", from)
}

pub fn rcpt_to_command(recipient: &str) -> Result<Vec<u8>, SmtpError> {
    envelope_command(b"RCPT TO:", recipient)
}

pub fn parse_reply(lines: &[&[u8]]) -> Result<SmtpReply, SmtpError> {
    if lines.is_empty() {
        return Err(SmtpError::MissingFinalLine);
    }
    let mut code = None;
    let mut text = Vec::with_capacity(lines.len());
    for (i, raw) in lines.iter().enumerate() {
        if raw.len() > MAX_LINE {
            return Err(SmtpError::LineTooLong);
        }
        let raw = raw.strip_suffix(b"\r\n").unwrap_or(raw);
        if raw.len() < 4
            || !raw[..3].iter().all(u8::is_ascii_digit)
            || (raw[3] != b' ' && raw[3] != b'-')
        {
            return Err(SmtpError::InvalidReply);
        }
        let found = std::str::from_utf8(&raw[..3])
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or(SmtpError::InvalidReply)?;
        if code.is_some_and(|c| c != found) {
            return Err(SmtpError::InconsistentCode);
        }
        code = Some(found);
        text.push(String::from_utf8_lossy(&raw[4..]).into_owned());
        if (raw[3] == b' ') != (i + 1 == lines.len()) {
            return Err(SmtpError::MissingFinalLine);
        }
    }
    Ok(SmtpReply {
        code: code.unwrap(),
        lines: text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiline_response() {
        let r = parse_reply(&[b"250-mail\r\n", b"250 SIZE 42\r\n"]).unwrap();
        assert_eq!(r.code, 250);
        assert_eq!(r.lines.len(), 2);
    }
    #[test]
    fn rejects_inconsistent_codes() {
        assert_eq!(
            parse_reply(&[b"250-a", b"550 b"]),
            Err(SmtpError::InconsistentCode)
        );
    }

    #[test]
    fn credential_ceilings_are_enforced_before_base64_expansion() {
        assert_eq!(validate_username("alice"), Ok(()));
        assert_eq!(validate_password("s3cr3t"), Ok(()));
        assert_eq!(validate_username(&"a".repeat(MAX_CREDENTIAL_LEN)), Ok(()));
        assert_eq!(
            validate_username(&"a".repeat(MAX_CREDENTIAL_LEN + 1)),
            Err(SmtpError::InvalidCredential)
        );
        assert_eq!(
            validate_password(&"a".repeat(MAX_CREDENTIAL_LEN + 1)),
            Err(SmtpError::InvalidCredential)
        );
        for value in ["", "control\r\nbyte", "tab\there"] {
            assert_eq!(validate_username(value), Err(SmtpError::InvalidCredential));
            assert_eq!(validate_password(value), Err(SmtpError::InvalidCredential));
        }
        assert!(expanded_auth_line_len(MAX_CREDENTIAL_LEN) <= MAX_AUTH_LINE);
        assert!(expanded_auth_line_len(MAX_CREDENTIAL_LEN + 1) > MAX_AUTH_LINE);
        assert_eq!(expanded_auth_line_len(0), 0);
        assert_eq!(expanded_auth_line_len(3), 4);
        assert_eq!(expanded_auth_line_len(4), 8);
    }

    #[test]
    fn envelope_lines_fit_the_frozen_budget() {
        assert_eq!(
            mail_from_command("from@postman.i2p").unwrap(),
            b"MAIL FROM:<from@postman.i2p>\r\n"
        );
        assert_eq!(
            rcpt_to_command("to@postman.i2p").unwrap(),
            b"RCPT TO:<to@postman.i2p>\r\n"
        );
        // The largest address that still fits, and the first that does not.
        let overhead = "MAIL FROM:<>\r\n".len();
        let max_address = MAX_ENVELOPE_LINE - overhead;
        assert_eq!(
            mail_from_command(&"a".repeat(max_address)).unwrap().len(),
            MAX_ENVELOPE_LINE
        );
        assert_eq!(
            mail_from_command(&"a".repeat(max_address + 1)),
            Err(SmtpError::InvalidEnvelope)
        );
        assert_eq!(
            rcpt_to_command(&"a".repeat(MAX_ENVELOPE_LINE)),
            Err(SmtpError::InvalidEnvelope)
        );
        for value in [
            "",
            "with space",
            "cr\r\nlf",
            "<already@bracketed>",
            "trailing@angle>",
            "second<recipient@i2p",
        ] {
            assert_eq!(
                rcpt_to_command(value),
                Err(SmtpError::InvalidEnvelope),
                "{value:?}"
            );
        }
    }
}
