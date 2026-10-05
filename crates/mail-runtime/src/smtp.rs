//! SMTP submission orchestration over the single authorized transport seam.
use crate::transport::{
    self, ByteStream, LineError, MailTransport, OperationControl, TransportError,
};
use i2pr_mail_domain::{SubmissionProgress, SubmissionState};
use i2pr_mail_proto::smtp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmitError {
    Transport,
    Protocol,
    Authentication,
    SizeLimit,
    RecipientRejected,
    OutboxOwned,
    Store,
    Cancelled,
    Timeout,
}

pub struct SmtpSubmission<'a> {
    pub account_id: &'a str,
    pub outbox_id: &'a str,
    pub from: &'a str,
    pub recipients: &'a [(String, String)],
    pub username: &'a str,
    pub password: &'a str,
    pub raw: &'a [u8],
    pub control: OperationControl,
}

/// Transport-neutral line failures become SMTP-typed errors at this boundary.
fn map_line_error(error: LineError) -> SubmitError {
    match error {
        LineError::Timeout => SubmitError::Timeout,
        LineError::Cancelled => SubmitError::Cancelled,
        LineError::TooLong => SubmitError::Protocol,
        LineError::Io | LineError::Closed => SubmitError::Transport,
    }
}

fn read_line(stream: &mut dyn ByteStream, out: &mut Vec<u8>) -> Result<(), SubmitError> {
    transport::read_line(stream, out).map_err(map_line_error)
}

fn smtp_reply(
    stream: &mut dyn ByteStream,
) -> Result<i2pr_mail_proto::smtp::SmtpReply, SubmitError> {
    let mut lines: Vec<Vec<u8>> = Vec::new();
    loop {
        let mut line = Vec::new();
        read_line(stream, &mut line)?;
        if lines.len() >= 100 {
            return Err(SubmitError::Protocol);
        }
        let final_line = line.get(3) == Some(&b' ');
        lines.push(line);
        if final_line {
            break;
        }
    }
    let refs: Vec<&[u8]> = lines.iter().map(Vec::as_slice).collect();
    i2pr_mail_proto::smtp::parse_reply(&refs).map_err(|_| SubmitError::Protocol)
}
fn smtp_command(
    stream: &mut dyn ByteStream,
    command: &[u8],
) -> Result<i2pr_mail_proto::smtp::SmtpReply, SubmitError> {
    stream.write_all(command).map_err(map_submit_transport)?;
    stream.write_all(b"\r\n").map_err(map_submit_transport)?;
    smtp_reply(stream)
}

/// Sends a line the protocol crate already bounded and CRLF-terminated.
fn smtp_line(
    stream: &mut dyn ByteStream,
    line: &[u8],
) -> Result<i2pr_mail_proto::smtp::SmtpReply, SubmitError> {
    stream.write_all(line).map_err(map_submit_transport)?;
    smtp_reply(stream)
}
fn map_submit_transport(error: TransportError) -> SubmitError {
    match error {
        TransportError::Timeout => SubmitError::Timeout,
        TransportError::Cancelled => SubmitError::Cancelled,
        _ => SubmitError::Transport,
    }
}
fn dot_stuff(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 16);
    let mut line_start = true;
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            b'\r' if raw.get(i + 1) == Some(&b'\n') => {
                out.extend_from_slice(b"\r\n");
                i += 2;
                line_start = true;
            }
            b'\r' | b'\n' => {
                out.extend_from_slice(b"\r\n");
                i += 1;
                line_start = true;
            }
            b'.' if line_start => {
                out.extend_from_slice(b"..");
                i += 1;
                line_start = false;
            }
            byte => {
                out.push(byte);
                i += 1;
                line_start = false;
            }
        }
    }
    if !line_start {
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}

/// Submit a previously composed RFC entity. The entity is durably queued before
/// network I/O. `DataMayHaveStarted` is persisted before DATA bytes are written and
/// is never automatically retried after transport ambiguity.
pub fn submit_smtp(
    transport: &dyn MailTransport,
    store: &mut i2pr_mail_store::Store,
    request: SmtpSubmission<'_>,
) -> Result<(), SubmitError> {
    let SmtpSubmission {
        account_id,
        outbox_id,
        from,
        recipients,
        username,
        password,
        raw,
        control,
    } = request;
    control.check().map_err(map_submit_transport)?;
    if recipients.is_empty() {
        return Err(SubmitError::RecipientRejected);
    }
    // Credentials and every envelope line are bounded against the frozen SMTP
    // ceilings before a stream is opened, a command buffer is formatted, or a
    // single base64 byte is produced.
    smtp::validate_username(username).map_err(|_| SubmitError::Authentication)?;
    smtp::validate_password(password).map_err(|_| SubmitError::Authentication)?;
    let mail_from_line = smtp::mail_from_command(from).map_err(|_| SubmitError::Authentication)?;
    let recipient_lines = recipients
        .iter()
        .map(|(address, _)| smtp::rcpt_to_command(address))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SubmitError::RecipientRejected)?;
    for len in [username.len(), password.len()] {
        if smtp::expanded_auth_line_len(len) > smtp::MAX_AUTH_LINE {
            return Err(SubmitError::Authentication);
        }
    }
    store
        .queue_outbox_full(outbox_id, from, raw, recipients)
        .map_err(|_| SubmitError::Store)?;
    let mut stream = transport
        .open(i2pr_mail_domain::MailService::Smtp, control.clone())
        .map_err(map_submit_transport)?;
    if smtp_reply(stream.as_mut())?.code != 220 {
        return Err(SubmitError::Protocol);
    }
    let ehlo = smtp_command(stream.as_mut(), b"EHLO i2pmail.invalid")?;
    if ehlo.code != 250 {
        return Err(SubmitError::Protocol);
    }
    let mut size_limit = None;
    let mut auth_login = false;
    for line in &ehlo.lines {
        let mut fields = line.split_ascii_whitespace();
        match fields.next().unwrap_or("").to_ascii_uppercase().as_str() {
            "SIZE" => size_limit = fields.next().and_then(|s| s.parse::<usize>().ok()),
            "AUTH" => auth_login = fields.any(|s| s.eq_ignore_ascii_case("LOGIN")),
            _ => {}
        }
    }
    if size_limit.is_some_and(|n| raw.len() > n) {
        let _ = store.claim_outbox(outbox_id);
        let _ = store.set_outbox_stage(
            outbox_id,
            SubmissionProgress::NotStarted,
            SubmissionState::FailedSafeToRetry,
        );
        return Err(SubmitError::SizeLimit);
    }
    if !auth_login {
        return Err(SubmitError::Authentication);
    }
    if smtp_command(stream.as_mut(), b"AUTH LOGIN")?.code != 334 {
        return Err(SubmitError::Authentication);
    }
    use base64::Engine;
    let encoded_user = base64::engine::general_purpose::STANDARD.encode(username.as_bytes());
    if encoded_user.len() > smtp::MAX_AUTH_LINE {
        return Err(SubmitError::Authentication);
    }
    if smtp_command(stream.as_mut(), encoded_user.as_bytes())?.code != 334 {
        return Err(SubmitError::Authentication);
    }
    let encoded_password = base64::engine::general_purpose::STANDARD.encode(password.as_bytes());
    if encoded_password.len() > smtp::MAX_AUTH_LINE {
        return Err(SubmitError::Authentication);
    }
    if smtp_command(stream.as_mut(), encoded_password.as_bytes())?.code != 235 {
        return Err(SubmitError::Authentication);
    }
    if !store
        .claim_outbox(outbox_id)
        .map_err(|_| SubmitError::Store)?
    {
        return Err(SubmitError::OutboxOwned);
    }
    if smtp_line(stream.as_mut(), &mail_from_line)?.code != 250 {
        let _ = store.set_outbox_stage(
            outbox_id,
            SubmissionProgress::NotStarted,
            SubmissionState::FailedSafeToRetry,
        );
        return Err(SubmitError::Protocol);
    }
    for command in &recipient_lines {
        if !matches!(smtp_line(stream.as_mut(), command)?.code, 250 | 251) {
            let _ = store.set_outbox_stage(
                outbox_id,
                SubmissionProgress::NotStarted,
                SubmissionState::FailedSafeToRetry,
            );
            return Err(SubmitError::RecipientRejected);
        }
    }
    control.check().map_err(map_submit_transport)?;
    if smtp_command(stream.as_mut(), b"DATA")?.code != 354 {
        let _ = store.set_outbox_stage(
            outbox_id,
            SubmissionProgress::NotStarted,
            SubmissionState::FailedSafeToRetry,
        );
        return Err(SubmitError::Protocol);
    }
    // Durable progress is recorded before any DATA byte is written.
    store
        .set_outbox_stage(
            outbox_id,
            SubmissionProgress::DataMayHaveStarted,
            SubmissionState::Submitting,
        )
        .map_err(|_| SubmitError::Store)?;
    let data = dot_stuff(raw);
    if let Err(error) = stream.write_all(&data) {
        let _ = store.set_outbox_stage(
            outbox_id,
            SubmissionProgress::DataMayHaveStarted,
            SubmissionState::DeliveryUnknown,
        );
        return Err(map_submit_transport(error));
    }
    let accepted = match smtp_reply(stream.as_mut()) {
        Ok(reply) => reply.code == 250,
        Err(error) => {
            let _ = store.set_outbox_stage(
                outbox_id,
                SubmissionProgress::DataMayHaveStarted,
                SubmissionState::DeliveryUnknown,
            );
            return Err(error);
        }
    };
    if !accepted {
        store
            .set_outbox_stage(
                outbox_id,
                SubmissionProgress::NotStarted,
                SubmissionState::FailedSafeToRetry,
            )
            .map_err(|_| SubmitError::Store)?;
        return Err(SubmitError::Protocol);
    }
    if !store
        .finalize_sent(outbox_id, account_id, &format!("sent-{outbox_id}"))
        .map_err(|_| SubmitError::Store)?
    {
        return Err(SubmitError::Store);
    }
    let _ = smtp_command(stream.as_mut(), b"QUIT");
    Ok(())
}
