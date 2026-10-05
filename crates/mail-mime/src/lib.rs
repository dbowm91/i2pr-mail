//! MIME boundary. Raw entities remain canonical; parsed views are bounded metadata only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawEntity(Vec<u8>);
impl RawEntity {
    pub const MAX_BYTES: usize = 25 * 1024 * 1024;
    pub fn new(bytes: Vec<u8>) -> Result<Self, MimeError> {
        if bytes.len() > Self::MAX_BYTES {
            Err(MimeError::TooLarge)
        } else {
            Ok(Self(bytes))
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MimeError {
    TooLarge,
    Malformed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedView {
    pub subject: Option<String>,
    pub text_plain: Option<String>,
    pub html_present: bool,
    pub attachment_count: usize,
}

/// Parse with the reviewed third-party parser while retaining the caller's exact
/// entity separately. The configured raw size ceiling bounds parser input.
pub fn parse(raw: &RawEntity) -> Result<ParsedView, MimeError> {
    let message = mail_parser::MessageParser::default()
        .parse(raw.as_bytes())
        .ok_or(MimeError::Malformed)?;
    let subject = message.subject().map(|s| s.to_string());
    let text_plain = message.body_text(0).map(|s| s.into_owned());
    Ok(ParsedView {
        subject,
        text_plain,
        html_present: !message.html_body.is_empty(),
        attachment_count: message.attachments.len(),
    })
}

/// Minimal bounded header extraction. The original bytes are never rewritten.
pub fn header_block(raw: &RawEntity) -> Result<&[u8], MimeError> {
    let end = raw
        .as_bytes()
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(MimeError::Malformed)?;
    if end > 256 * 1024 {
        return Err(MimeError::TooLarge);
    }
    Ok(&raw.as_bytes()[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_raw_bytes_and_bounds_headers() {
        let bytes = b"Subject: x\r\n\r\nbody".to_vec();
        let entity = RawEntity::new(bytes.clone()).unwrap();
        assert_eq!(entity.as_bytes(), bytes);
        assert_eq!(header_block(&entity).unwrap(), b"Subject: x");
        assert_eq!(
            RawEntity::new(vec![0; RawEntity::MAX_BYTES + 1]),
            Err(MimeError::TooLarge)
        );
    }

    #[test]
    fn parses_plain_text_without_replacing_raw_entity() {
        let raw =
            RawEntity::new(b"Subject: hello\r\nContent-Type: text/plain\r\n\r\nworld".to_vec())
                .unwrap();
        let view = parse(&raw).unwrap();
        assert_eq!(view.subject.as_deref(), Some("hello"));
        assert_eq!(view.text_plain.as_deref(), Some("world"));
        assert_eq!(raw.as_bytes().last(), Some(&b'd'));
    }
}
