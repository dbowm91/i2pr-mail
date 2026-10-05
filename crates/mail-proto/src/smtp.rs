pub const MAX_LINE: usize = 8192;
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
}
