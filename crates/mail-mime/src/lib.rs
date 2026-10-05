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

#[derive(Clone, Debug)]
pub struct ComposeInput<'a> {
    pub from: &'a str,
    pub to: &'a [&'a str],
    pub cc: &'a [&'a str],
    /// Envelope-only recipients; never copied to message headers.
    pub bcc: &'a [&'a str],
    pub subject: &'a str,
    pub text: &'a str,
    pub message_id: &'a str,
    pub unix_time_utc: i64,
    pub attachments: &'a [Attachment<'a>],
}

#[derive(Clone, Debug)]
pub struct Attachment<'a> {
    pub content_type: &'a str,
    pub filename: &'a str,
    pub bytes: &'a [u8],
}

/// Build a bounded text message. Callers retain `bcc` only in the SMTP envelope.
pub fn build_text(input: &ComposeInput<'_>) -> Result<RawEntity, MimeError> {
    use mail_builder::MessageBuilder;
    if input.bcc.len() > 100 || input.to.len() + input.cc.len() > 100 {
        return Err(MimeError::TooLarge);
    }
    if input.subject.len() > 16 * 1024
        || input.from.len() > 320
        || input.text.len() > RawEntity::MAX_BYTES
        || input.attachments.len() > 100
    {
        return Err(MimeError::TooLarge);
    }
    let attachment_bytes = input
        .attachments
        .iter()
        .try_fold(0usize, |sum, a| {
            sum.checked_add(a.bytes.len())
                .filter(|n| *n <= RawEntity::MAX_BYTES)
        })
        .ok_or(MimeError::TooLarge)?;
    if input.text.len().saturating_add(attachment_bytes) > RawEntity::MAX_BYTES {
        return Err(MimeError::TooLarge);
    }
    if [input.from, input.subject, input.message_id]
        .iter()
        .any(|s| s.contains('\r') || s.contains('\n'))
    {
        return Err(MimeError::Malformed);
    }
    if input
        .to
        .iter()
        .chain(input.cc)
        .chain(input.bcc)
        .any(|s| s.contains('\r') || s.contains('\n'))
    {
        return Err(MimeError::Malformed);
    }
    if input
        .to
        .iter()
        .chain(input.cc)
        .chain(input.bcc)
        .any(|s| s.len() > 320)
    {
        return Err(MimeError::TooLarge);
    }
    let mut builder = MessageBuilder::new()
        .from(input.from)
        .subject(input.subject)
        .message_id(input.message_id)
        .date(input.unix_time_utc)
        .text_body(input.text);
    for attachment in input.attachments {
        if attachment.filename.contains(['\r', '\n'])
            || attachment.content_type.contains(['\r', '\n'])
            || attachment.bytes.len() > RawEntity::MAX_BYTES
        {
            return Err(MimeError::Malformed);
        }
        builder = builder.attachment(
            attachment.content_type,
            attachment.filename,
            attachment.bytes,
        );
    }
    for address in input.to {
        builder = builder.to(*address);
    }
    for address in input.cc {
        builder = builder.cc(*address);
    }
    let bytes = builder.write_to_vec().map_err(|_| MimeError::Malformed)?;
    RawEntity::new(bytes)
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

    #[test]
    fn generated_message_uses_explicit_privacy_safe_headers_and_omits_bcc() {
        let entity = build_text(&ComposeInput {
            from: "a@postman.i2p",
            to: &["b@postman.i2p"],
            cc: &[],
            bcc: &["secret@postman.i2p"],
            subject: "hello",
            text: "body",
            message_id: "random@i2pmail.invalid",
            unix_time_utc: 1_700_000_000,
            attachments: &[],
        })
        .unwrap();
        let raw = String::from_utf8(entity.as_bytes().to_vec()).unwrap();
        assert!(raw.contains("Message-ID: <random@i2pmail.invalid>"));
        assert!(raw.contains("+0000"));
        assert!(!raw.contains("secret@postman.i2p"));
        assert!(!raw.to_ascii_lowercase().contains("user-agent:"));
        assert!(!raw.to_ascii_lowercase().contains("x-mailer:"));
        assert!(!raw.contains("localhost"));
        assert!(!raw.contains("codex-host-sentinel"));
        assert!(!raw.contains("codex-user-sentinel"));
        assert!(!raw.contains("router-v9"));
    }

    #[test]
    fn parses_multipart_and_attachment_metadata() {
        let raw = RawEntity::new(b"Subject: multipart\r\nContent-Type: multipart/mixed; boundary=xx\r\n\r\n--xx\r\nContent-Type: text/plain\r\n\r\nhello\r\n--xx\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=secret.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAQID\r\n--xx--\r\n".to_vec()).unwrap();
        let view = parse(&raw).unwrap();
        assert_eq!(view.text_plain.as_deref(), Some("hello"));
        assert_eq!(view.attachment_count, 1);
        assert_eq!(raw.as_bytes().last(), Some(&b'\n'));
    }

    #[test]
    fn rejects_header_injection_in_compose_fields() {
        let input = ComposeInput {
            from: "a@x\r\nBcc: injected@x",
            to: &[],
            cc: &[],
            bcc: &[],
            subject: "x",
            text: "body",
            message_id: "id@invalid",
            unix_time_utc: 1,
            attachments: &[],
        };
        assert_eq!(build_text(&input).unwrap_err(), MimeError::Malformed);
    }

    #[test]
    fn builds_attachment_without_adding_local_identity_headers() {
        let attachments = [Attachment {
            content_type: "application/octet-stream",
            filename: "data.bin",
            bytes: &[1, 2, 3],
        }];
        let entity = build_text(&ComposeInput {
            from: "sender@postman.i2p",
            to: &["to@postman.i2p"],
            cc: &[],
            bcc: &[],
            subject: "file",
            text: "see attached",
            message_id: "random@invalid",
            unix_time_utc: 1_700_000_000,
            attachments: &attachments,
        })
        .unwrap();
        let raw = String::from_utf8_lossy(entity.as_bytes()).to_ascii_lowercase();
        assert!(raw.contains("filename=data.bin") || raw.contains("filename=\"data.bin\""));
        assert!(!raw.contains("x-mailer:"));
        assert!(!raw.contains("user-agent:"));
    }

    #[test]
    fn unicode_subject_round_trips_through_mime_facade() {
        let raw = build_text(&ComposeInput {
            from: "sender@postman.i2p",
            to: &["to@postman.i2p"],
            cc: &[],
            bcc: &[],
            subject: "Héllo 世界",
            text: "body",
            message_id: "random@invalid",
            unix_time_utc: 1_700_000_000,
            attachments: &[],
        })
        .unwrap();
        assert_eq!(parse(&raw).unwrap().subject.as_deref(), Some("Héllo 世界"));
    }

    #[test]
    fn compose_allows_multiline_body_without_allowing_header_injection() {
        let raw = build_text(&ComposeInput {
            from: "sender@postman.i2p",
            to: &[],
            cc: &[],
            bcc: &[],
            subject: "body lines",
            text: "line one\r\nline two",
            message_id: "random@invalid",
            unix_time_utc: 1_700_000_000,
            attachments: &[],
        })
        .unwrap();
        let body = b"line one\r\nline two";
        assert!(raw.as_bytes().windows(body.len()).any(|w| w == body));
    }
}
