//! Runtime owns authorized transport and orchestration; protocol crates stay sans-I/O.
pub trait MailTransport: Send + Sync {
    /// Implementations must bound connect and stream I/O by this shared deadline
    /// and promptly close an owned stream when its cancellation flag is set.
    fn open(
        &self,
        service: i2pr_mail_domain::MailService,
        control: OperationControl,
    ) -> Result<Box<dyn ByteStream>, TransportError>;
}
pub trait ByteStream: Send {
    /// Must observe the `OperationControl` retained by `MailTransport::open` and
    /// return TransportError::Cancelled/Timeout instead of blocking past it.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), TransportError>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    Unavailable,
    Denied,
    Closed,
    Io,
    Timeout,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct OperationControl {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    deadline: std::time::Instant,
}
impl OperationControl {
    pub fn new(timeout: std::time::Duration) -> Self {
        Self {
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            deadline: std::time::Instant::now() + timeout,
        }
    }
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub fn check(&self) -> Result<(), SyncError> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            Err(SyncError::Cancelled)
        } else if std::time::Instant::now() >= self.deadline {
            Err(SyncError::Timeout)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncError {
    Transport,
    Protocol,
    Authentication,
    Limits,
    Store,
    Cancelled,
    Timeout,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub remote_count: usize,
    pub headers_cached: usize,
    pub bodies_cached: usize,
}

fn write_command(stream: &mut dyn ByteStream, command: &[u8]) -> Result<(), SyncError> {
    stream.write_all(command).map_err(map_sync_transport)?;
    stream.write_all(b"\r\n").map_err(map_sync_transport)
}
fn map_sync_transport(error: TransportError) -> SyncError {
    match error {
        TransportError::Timeout => SyncError::Timeout,
        TransportError::Cancelled => SyncError::Cancelled,
        _ => SyncError::Transport,
    }
}
fn read_line(stream: &mut dyn ByteStream, out: &mut Vec<u8>) -> Result<(), SyncError> {
    out.clear();
    loop {
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => return Err(SyncError::Transport),
            Ok(_) => {}
            Err(error) => return Err(map_sync_transport(error)),
        }
        out.push(byte[0]);
        if out.len() > i2pr_mail_proto::pop3::MAX_LINE {
            return Err(SyncError::Limits);
        }
        if out.ends_with(b"\r\n") {
            return Ok(());
        }
    }
}
fn expect_ok(stream: &mut dyn ByteStream) -> Result<(), SyncError> {
    if read_status(stream)? {
        Ok(())
    } else {
        Err(SyncError::Protocol)
    }
}
fn read_status(stream: &mut dyn ByteStream) -> Result<bool, SyncError> {
    let mut line = Vec::new();
    read_line(stream, &mut line)?;
    match i2pr_mail_proto::pop3::parse_status(&line).map_err(|_| SyncError::Protocol)? {
        i2pr_mail_proto::pop3::Pop3Reply::Status { positive, .. } => Ok(positive),
        _ => Err(SyncError::Protocol),
    }
}
fn read_multiline(stream: &mut dyn ByteStream) -> Result<Vec<Vec<u8>>, SyncError> {
    let mut rows = Vec::new();
    let mut total = 0usize;
    loop {
        let mut line = Vec::new();
        read_line(stream, &mut line)?;
        total += line.len();
        if total > 16 * 1024 * 1024 || rows.len() > 100_000 {
            return Err(SyncError::Limits);
        }
        let terminal = line == b".\r\n";
        rows.push(line);
        if terminal {
            break;
        }
    }
    Ok(rows)
}
fn unstuff(rows: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        if row == b".\r\n" {
            break;
        }
        let row = if row.starts_with(b"..") {
            &row[1..]
        } else {
            &row[..]
        };
        out.extend_from_slice(row);
    }
    out
}
fn message_key(account: &str, uidl: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(account.as_bytes());
    hash.update([0]);
    hash.update(uidl.as_bytes());
    let mut s = String::from("remote-");
    for b in hash.finalize() {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Header-first UIDL reconciliation over the only authorized transport seam.
/// Credentials are borrowed ephemerally and never included in errors or reports.
pub fn sync_pop3(
    transport: &dyn MailTransport,
    store: &mut i2pr_mail_store::Store,
    account: &str,
    username: &str,
    password: &str,
) -> Result<SyncReport, SyncError> {
    let control = OperationControl::new(std::time::Duration::from_secs(300));
    sync_pop3_with_control(transport, store, account, username, password, control)
}

pub fn sync_pop3_with_control(
    transport: &dyn MailTransport,
    store: &mut i2pr_mail_store::Store,
    account: &str,
    username: &str,
    password: &str,
    control: OperationControl,
) -> Result<SyncReport, SyncError> {
    control.check()?;
    let mut stream = transport
        .open(i2pr_mail_domain::MailService::Pop3, control.clone())
        .map_err(map_sync_transport)?;
    expect_ok(stream.as_mut())?;
    write_command(stream.as_mut(), b"CAPA")?;
    if read_status(stream.as_mut())? {
        let _ = read_multiline(stream.as_mut())?;
    }
    // POP3 credentials are line atoms and may not contain protocol delimiters.
    if username.bytes().any(|b| !(0x20..=0x7e).contains(&b))
        || password.bytes().any(|b| !(0x20..=0x7e).contains(&b))
    {
        return Err(SyncError::Authentication);
    }
    write_command(stream.as_mut(), format!("USER {username}").as_bytes())?;
    expect_ok(stream.as_mut())?;
    write_command(stream.as_mut(), format!("PASS {password}").as_bytes())?;
    expect_ok(stream.as_mut()).map_err(|_| SyncError::Authentication)?;
    write_command(stream.as_mut(), b"STAT")?;
    expect_ok(stream.as_mut())?;
    write_command(stream.as_mut(), b"UIDL")?;
    expect_ok(stream.as_mut())?;
    let uidl_rows = read_multiline(stream.as_mut())?;
    let entries =
        i2pr_mail_proto::pop3::parse_uidl_listing(&uidl_rows).map_err(|_| SyncError::Protocol)?;
    if entries.len() > 100_000 {
        return Err(SyncError::Limits);
    }
    write_command(stream.as_mut(), b"LIST")?;
    expect_ok(stream.as_mut())?;
    let list_rows = read_multiline(stream.as_mut())?;
    let sizes =
        i2pr_mail_proto::pop3::parse_list_listing(&list_rows).map_err(|_| SyncError::Protocol)?;
    let sizes: std::collections::HashMap<u32, u64> = sizes.into_iter().collect();
    let remote_ordinals: std::collections::HashMap<&str, u32> = entries
        .iter()
        .map(|e| (e.uidl.as_str(), e.ordinal))
        .collect();
    for (uidl, state) in store
        .deletion_states(account)
        .map_err(|_| SyncError::Store)?
    {
        if !remote_ordinals.contains_key(uidl.as_str()) {
            store
                .set_receive_state(account, &uidl, "RemoteDeletionCommitted")
                .map_err(|_| SyncError::Store)?;
        } else if state == "DeleteMarkedSession" {
            store
                .set_receive_state(account, &uidl, "DeletePending")
                .map_err(|_| SyncError::Store)?;
        }
    }
    let mut report = SyncReport {
        remote_count: entries.len(),
        ..Default::default()
    };
    for entry in entries {
        if !sizes.contains_key(&entry.ordinal) {
            return Err(SyncError::Protocol);
        }
        if let Some(existing) = store
            .message_by_uidl(account, &entry.uidl)
            .map_err(|_| SyncError::Store)?
        {
            if existing.receive_state == "DeletePending" {
                write_command(
                    stream.as_mut(),
                    format!("DELE {}", entry.ordinal).as_bytes(),
                )?;
                expect_ok(stream.as_mut())?;
                store
                    .set_receive_state(account, &entry.uidl, "DeleteMarkedSession")
                    .map_err(|_| SyncError::Store)?;
            }
            continue;
        }
        write_command(
            stream.as_mut(),
            format!("TOP {} 0", entry.ordinal).as_bytes(),
        )?;
        let (entity, complete) = if read_status(stream.as_mut())? {
            (unstuff(read_multiline(stream.as_mut())?), false)
        } else {
            write_command(
                stream.as_mut(),
                format!("RETR {}", entry.ordinal).as_bytes(),
            )?;
            expect_ok(stream.as_mut())?;
            (unstuff(read_multiline(stream.as_mut())?), true)
        };
        let state = if complete {
            "BodyCached"
        } else {
            "HeaderCached"
        };
        store
            .save_message(
                &message_key(account, &entry.uidl),
                account,
                Some(&entry.uidl),
                state,
                &entity,
            )
            .map_err(|_| SyncError::Store)?;
        report.headers_cached += usize::from(state == "HeaderCached");
        report.bodies_cached += usize::from(state == "BodyCached");
    }
    write_command(stream.as_mut(), b"QUIT")?;
    expect_ok(stream.as_mut())?;
    for (uidl, state) in store
        .deletion_states(account)
        .map_err(|_| SyncError::Store)?
    {
        if state == "DeleteMarkedSession" {
            store
                .set_receive_state(account, &uidl, "RemoteDeletionCommitted")
                .map_err(|_| SyncError::Store)?;
        }
    }
    Ok(report)
}

/// Explicit, bounded full-body fetch for an already-known opaque UIDL.
pub fn fetch_pop3_body(
    transport: &dyn MailTransport,
    store: &mut i2pr_mail_store::Store,
    account: &str,
    username: &str,
    password: &str,
    uidl: &str,
) -> Result<(), SyncError> {
    let control = OperationControl::new(std::time::Duration::from_secs(300));
    fetch_pop3_body_with_control(transport, store, account, username, password, uidl, control)
}

pub fn fetch_pop3_body_with_control(
    transport: &dyn MailTransport,
    store: &mut i2pr_mail_store::Store,
    account: &str,
    username: &str,
    password: &str,
    uidl: &str,
    control: OperationControl,
) -> Result<(), SyncError> {
    control.check()?;
    if username.bytes().any(|b| !(0x20..=0x7e).contains(&b))
        || password.bytes().any(|b| !(0x20..=0x7e).contains(&b))
    {
        return Err(SyncError::Authentication);
    }
    let mut stream = transport
        .open(i2pr_mail_domain::MailService::Pop3, control.clone())
        .map_err(map_sync_transport)?;
    expect_ok(stream.as_mut())?;
    write_command(stream.as_mut(), format!("USER {username}").as_bytes())?;
    expect_ok(stream.as_mut())?;
    write_command(stream.as_mut(), format!("PASS {password}").as_bytes())?;
    expect_ok(stream.as_mut()).map_err(|_| SyncError::Authentication)?;
    write_command(stream.as_mut(), b"UIDL")?;
    expect_ok(stream.as_mut())?;
    let rows = read_multiline(stream.as_mut())?;
    let entries =
        i2pr_mail_proto::pop3::parse_uidl_listing(&rows).map_err(|_| SyncError::Protocol)?;
    let ordinal = entries
        .iter()
        .find(|entry| entry.uidl == uidl)
        .map(|entry| entry.ordinal)
        .ok_or(SyncError::Protocol)?;
    write_command(stream.as_mut(), format!("RETR {ordinal}").as_bytes())?;
    expect_ok(stream.as_mut())?;
    let raw = unstuff(read_multiline(stream.as_mut())?);
    let existing = store
        .message_by_uidl(account, uidl)
        .map_err(|_| SyncError::Store)?
        .ok_or(SyncError::Store)?;
    store
        .save_message(&existing.id, account, Some(uidl), "BodyCached", &raw)
        .map_err(|_| SyncError::Store)?;
    write_command(stream.as_mut(), b"QUIT")?;
    expect_ok(stream.as_mut())?;
    Ok(())
}

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

fn smtp_reply(
    stream: &mut dyn ByteStream,
) -> Result<i2pr_mail_proto::smtp::SmtpReply, SubmitError> {
    let mut lines: Vec<Vec<u8>> = Vec::new();
    loop {
        let mut line = Vec::new();
        read_line(stream, &mut line).map_err(|e| match e {
            SyncError::Timeout => SubmitError::Timeout,
            SyncError::Cancelled => SubmitError::Cancelled,
            _ => SubmitError::Transport,
        })?;
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
/// network I/O. DATA stage 1 is persisted before bytes are written and never
/// automatically retried after transport ambiguity.
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
    control.check().map_err(|e| match e {
        SyncError::Cancelled => SubmitError::Cancelled,
        _ => SubmitError::Timeout,
    })?;
    if recipients.is_empty() {
        return Err(SubmitError::RecipientRejected);
    }
    if [from, username, password]
        .iter()
        .any(|s| s.bytes().any(|b| !(0x20..=0x7e).contains(&b)))
        || recipients
            .iter()
            .any(|(a, _)| a.bytes().any(|b| !(0x21..=0x7e).contains(&b)))
    {
        return Err(SubmitError::Authentication);
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
        let _ = store.set_outbox_stage(outbox_id, 0, "FailedSafeToRetry");
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
    if smtp_command(stream.as_mut(), encoded_user.as_bytes())?.code != 334 {
        return Err(SubmitError::Authentication);
    }
    let encoded_password = base64::engine::general_purpose::STANDARD.encode(password.as_bytes());
    if smtp_command(stream.as_mut(), encoded_password.as_bytes())?.code != 235 {
        return Err(SubmitError::Authentication);
    }
    if !store
        .claim_outbox(outbox_id)
        .map_err(|_| SubmitError::Store)?
    {
        return Err(SubmitError::OutboxOwned);
    }
    let mail_from = format!("MAIL FROM:<{from}>");
    if smtp_command(stream.as_mut(), mail_from.as_bytes())?.code != 250 {
        let _ = store.set_outbox_stage(outbox_id, 0, "FailedSafeToRetry");
        return Err(SubmitError::Protocol);
    }
    for (recipient, _) in recipients {
        let command = format!("RCPT TO:<{recipient}>");
        if !matches!(
            smtp_command(stream.as_mut(), command.as_bytes())?.code,
            250 | 251
        ) {
            let _ = store.set_outbox_stage(outbox_id, 0, "FailedSafeToRetry");
            return Err(SubmitError::RecipientRejected);
        }
    }
    control.check().map_err(|e| match e {
        SyncError::Cancelled => SubmitError::Cancelled,
        _ => SubmitError::Timeout,
    })?;
    if smtp_command(stream.as_mut(), b"DATA")?.code != 354 {
        let _ = store.set_outbox_stage(outbox_id, 0, "FailedSafeToRetry");
        return Err(SubmitError::Protocol);
    }
    store
        .set_outbox_stage(outbox_id, 1, "Submitting")
        .map_err(|_| SubmitError::Store)?;
    let data = dot_stuff(raw);
    if let Err(error) = stream.write_all(&data) {
        let _ = store.set_outbox_stage(outbox_id, 1, "DeliveryUnknown");
        return Err(map_submit_transport(error));
    }
    let accepted = match smtp_reply(stream.as_mut()) {
        Ok(reply) => reply.code == 250,
        Err(error) => {
            let _ = store.set_outbox_stage(outbox_id, 1, "DeliveryUnknown");
            return Err(error);
        }
    };
    if !accepted {
        store
            .set_outbox_stage(outbox_id, 0, "FailedSafeToRetry")
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

pub const MAX_ACTIVE_CONTENT_HANDLES: usize = 32;
pub const MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CONTENT_READ: usize = 64 * 1024;
pub const MAX_SESSION_REQUESTS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(String);
impl RequestId {
    pub fn new(value: impl Into<String>) -> Result<Self, BackendError> {
        let v = value.into();
        if v.is_empty() || v.len() > 128 || !v.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
            Err(BackendError::InvalidRequest)
        } else {
            Ok(Self(v))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct Credentials {
    username: String,
    password: String,
}
impl Credentials {
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}
pub trait CredentialSource: Send + Sync {
    fn credentials(
        &self,
        account: &str,
        service: i2pr_mail_domain::MailService,
    ) -> Result<Credentials, BackendError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    InvalidRequest,
    NotFound,
    Capacity,
    DeliveryUnknown,
    Store,
    Transport,
    Protocol,
    Authentication,
    Cancelled,
    Timeout,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageSummary {
    pub id: String,
    pub uidl: Option<String>,
    pub state: String,
    pub locally_deleted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendHealth {
    pub ready: bool,
    pub active_content_handles: usize,
    pub schema_version: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboxStatus {
    pub state: String,
    pub stage: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryResolution {
    ConfirmSent,
    PermitRetry,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentHandle(u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendEventKind {
    Started,
    SyncFinished(SyncReport),
    Completed,
    Failed(BackendError),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendEvent {
    pub request_id: RequestId,
    pub kind: BackendEventKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationResult<T> {
    pub request_id: RequestId,
    pub value: Result<T, BackendError>,
    pub events: Vec<BackendEvent>,
}

/// Single mutable lifecycle owner for the frontend-neutral backend surface.
pub struct BackendService {
    store: i2pr_mail_store::Store,
    transport: std::sync::Arc<dyn MailTransport>,
    account: String,
    handles: std::collections::HashMap<ContentHandle, Vec<u8>>,
    content_bytes: usize,
    next_handle: u64,
    seen_requests: std::collections::HashSet<RequestId>,
}
impl BackendService {
    pub fn open(
        root: impl AsRef<std::path::Path>,
        account: impl Into<String>,
        transport: std::sync::Arc<dyn MailTransport>,
    ) -> Result<Self, BackendError> {
        let account = account.into();
        i2pr_mail_domain::AccountId::new(account.clone())
            .map_err(|_| BackendError::InvalidRequest)?;
        let mut store = i2pr_mail_store::Store::open(root).map_err(|_| BackendError::Store)?;
        store
            .recover_submitting()
            .map_err(|_| BackendError::Store)?;
        Ok(Self {
            store,
            transport,
            account,
            handles: Default::default(),
            content_bytes: 0,
            next_handle: 1,
            seen_requests: Default::default(),
        })
    }
    pub fn sync(
        &mut self,
        request_id: RequestId,
        source: &dyn CredentialSource,
        control: OperationControl,
    ) -> OperationResult<SyncReport> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let start = BackendEvent {
            request_id: request_id.clone(),
            kind: BackendEventKind::Started,
        };
        let result = source
            .credentials(&self.account, i2pr_mail_domain::MailService::Pop3)
            .and_then(|c| {
                sync_pop3_with_control(
                    self.transport.as_ref(),
                    &mut self.store,
                    &self.account,
                    &c.username,
                    &c.password,
                    control,
                )
                .map_err(map_sync_backend)
            });
        let final_kind = match &result {
            Ok(report) => BackendEventKind::SyncFinished(report.clone()),
            Err(e) => BackendEventKind::Failed(e.clone()),
        };
        OperationResult {
            request_id: request_id.clone(),
            value: result,
            events: vec![
                start,
                BackendEvent {
                    request_id,
                    kind: final_kind,
                },
            ],
        }
    }
    pub fn service_profile(&self) -> i2pr_mail_domain::ServiceProfile {
        i2pr_mail_domain::ServiceProfile::default()
    }
    pub fn health(&mut self, request_id: RequestId) -> OperationResult<BackendHealth> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .schema_version()
            .map(|schema_version| BackendHealth {
                ready: true,
                active_content_handles: self.handles.len(),
                schema_version,
            })
            .map_err(|_| BackendError::Store);
        self.result(request_id, result)
    }
    pub fn inspect_outbox(
        &mut self,
        request_id: RequestId,
        outbox_id: &str,
    ) -> OperationResult<OutboxStatus> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .outbox_state(outbox_id)
            .map_err(|_| BackendError::Store)
            .and_then(|v| {
                v.map(|(state, stage)| OutboxStatus { state, stage })
                    .ok_or(BackendError::NotFound)
            });
        self.result(request_id, result)
    }
    pub fn resolve_delivery_unknown(
        &mut self,
        request_id: RequestId,
        outbox_id: &str,
        resolution: DeliveryResolution,
    ) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .resolve_delivery_unknown(
                outbox_id,
                &self.account,
                matches!(resolution, DeliveryResolution::ConfirmSent),
            )
            .map_err(|_| BackendError::Store)
            .and_then(|ok| {
                if ok {
                    Ok(())
                } else {
                    Err(BackendError::NotFound)
                }
            });
        self.result(request_id, result)
    }
    pub fn list_messages(&mut self, request_id: RequestId) -> OperationResult<Vec<MessageSummary>> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .messages_for_account(&self.account)
            .and_then(|rows| {
                rows.into_iter()
                    .map(|m| {
                        Ok(MessageSummary {
                            id: m.id.clone(),
                            uidl: m.uidl,
                            state: m.receive_state,
                            locally_deleted: self.store.is_locally_deleted(&m.id)?,
                        })
                    })
                    .collect::<Result<Vec<_>, i2pr_mail_store::StoreError>>()
            })
            .map_err(|_| BackendError::Store);
        self.result(request_id, result)
    }
    pub fn open_message(
        &mut self,
        request_id: RequestId,
        message_id: &str,
    ) -> OperationResult<ContentHandle> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = (|| {
            if self.handles.len() >= MAX_ACTIVE_CONTENT_HANDLES {
                return Err(BackendError::Capacity);
            }
            let data = self
                .store
                .raw_for_message(message_id)
                .map_err(|_| BackendError::Store)?
                .ok_or(BackendError::NotFound)?;
            if data.len() > MAX_CONTENT_BYTES - self.content_bytes {
                return Err(BackendError::Capacity);
            }
            let handle = ContentHandle(self.next_handle);
            self.next_handle = self
                .next_handle
                .checked_add(1)
                .ok_or(BackendError::Capacity)?;
            self.content_bytes += data.len();
            self.handles.insert(handle, data);
            Ok(handle)
        })();
        self.result(request_id, result)
    }
    pub fn read_content(
        &mut self,
        request_id: RequestId,
        handle: ContentHandle,
        offset: usize,
        max_bytes: usize,
    ) -> OperationResult<Vec<u8>> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = if max_bytes > MAX_CONTENT_READ {
            Err(BackendError::Capacity)
        } else if let Some(data) = self.handles.get(&handle) {
            if offset > data.len() {
                Err(BackendError::InvalidRequest)
            } else {
                Ok(data[offset..offset + max_bytes.min(data.len() - offset)].to_vec())
            }
        } else {
            Err(BackendError::NotFound)
        };
        self.result(request_id, result)
    }
    pub fn release_content(
        &mut self,
        request_id: RequestId,
        handle: ContentHandle,
    ) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .handles
            .remove(&handle)
            .map(|data| {
                self.content_bytes -= data.len();
            })
            .ok_or(BackendError::NotFound);
        self.result(request_id, result)
    }
    pub fn save_draft(
        &mut self,
        request_id: RequestId,
        draft_id: &str,
        raw: &[u8],
    ) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .save_draft(draft_id, raw)
            .map(|_| ())
            .map_err(|_| BackendError::Store);
        self.result(request_id, result)
    }
    pub fn delete_draft(&mut self, request_id: RequestId, draft_id: &str) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .delete_draft(draft_id)
            .map_err(|_| BackendError::Store)
            .and_then(|removed| {
                if removed {
                    Ok(())
                } else {
                    Err(BackendError::NotFound)
                }
            });
        self.result(request_id, result)
    }
    pub fn delete_local(&mut self, request_id: RequestId, message_id: &str) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .local_delete(message_id)
            .map_err(|_| BackendError::Store)
            .and_then(|removed| {
                if removed {
                    Ok(())
                } else {
                    Err(BackendError::NotFound)
                }
            });
        self.result(request_id, result)
    }
    pub fn queue_send(
        &mut self,
        request_id: RequestId,
        outbox_id: &str,
        from: &str,
        recipients: &[(String, String)],
        raw: &[u8],
    ) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .queue_outbox_full(outbox_id, from, raw, recipients)
            .map(|_| ())
            .map_err(|_| BackendError::Store);
        self.result(request_id, result)
    }
    pub fn send_queued(
        &mut self,
        request_id: RequestId,
        outbox_id: &str,
        source: &dyn CredentialSource,
        control: OperationControl,
    ) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = (|| {
            let state = self
                .store
                .outbox_state(outbox_id)
                .map_err(|_| BackendError::Store)?
                .ok_or(BackendError::NotFound)?;
            if state.0 == "DeliveryUnknown" {
                return Err(BackendError::DeliveryUnknown);
            }
            let from = self
                .store
                .outbox_sender(outbox_id)
                .map_err(|_| BackendError::Store)?
                .ok_or(BackendError::NotFound)?;
            let recipients = self
                .store
                .outbox_recipients(outbox_id)
                .map_err(|_| BackendError::Store)?;
            let raw = self
                .store
                .outbox_raw(outbox_id)
                .map_err(|_| BackendError::Store)?
                .ok_or(BackendError::NotFound)?;
            let c = source.credentials(&self.account, i2pr_mail_domain::MailService::Smtp)?;
            submit_smtp(
                self.transport.as_ref(),
                &mut self.store,
                SmtpSubmission {
                    account_id: &self.account,
                    outbox_id,
                    from: &from,
                    recipients: &recipients,
                    username: &c.username,
                    password: &c.password,
                    raw: &raw,
                    control,
                },
            )
            .map_err(map_submit_backend)
        })();
        self.result(request_id, result)
    }
    pub fn request_remote_delete(
        &mut self,
        request_id: RequestId,
        uidl: &str,
    ) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = self
            .store
            .mark_delete_pending(&self.account, uidl)
            .map_err(|_| BackendError::Store)
            .and_then(|changed| {
                if changed {
                    Ok(())
                } else {
                    Err(BackendError::NotFound)
                }
            });
        self.result(request_id, result)
    }
    pub fn fetch_body(
        &mut self,
        request_id: RequestId,
        uidl: &str,
        source: &dyn CredentialSource,
        control: OperationControl,
    ) -> OperationResult<()> {
        if let Err(e) = self.begin_request(&request_id) {
            return self.result(request_id, Err(e));
        }
        let result = source
            .credentials(&self.account, i2pr_mail_domain::MailService::Pop3)
            .and_then(|c| {
                fetch_pop3_body_with_control(
                    self.transport.as_ref(),
                    &mut self.store,
                    &self.account,
                    &c.username,
                    &c.password,
                    uidl,
                    control,
                )
                .map_err(map_sync_backend)
            });
        self.result(request_id, result)
    }
    pub fn shutdown(mut self) {
        self.handles.clear();
        self.content_bytes = 0;
    }
    fn result<T>(
        &self,
        request_id: RequestId,
        result: Result<T, BackendError>,
    ) -> OperationResult<T> {
        let kind = match &result {
            Ok(_) => BackendEventKind::Completed,
            Err(e) => BackendEventKind::Failed(e.clone()),
        };
        OperationResult {
            events: vec![BackendEvent {
                request_id: request_id.clone(),
                kind,
            }],
            request_id,
            value: result,
        }
    }
    fn begin_request(&mut self, id: &RequestId) -> Result<(), BackendError> {
        if self.seen_requests.contains(id) {
            return Err(BackendError::InvalidRequest);
        }
        if self.seen_requests.len() >= MAX_SESSION_REQUESTS {
            return Err(BackendError::Capacity);
        }
        self.seen_requests.insert(id.clone());
        Ok(())
    }
}
fn map_sync_backend(e: SyncError) -> BackendError {
    match e {
        SyncError::Authentication => BackendError::Authentication,
        SyncError::Store => BackendError::Store,
        SyncError::Cancelled => BackendError::Cancelled,
        SyncError::Timeout => BackendError::Timeout,
        SyncError::Protocol | SyncError::Limits => BackendError::Protocol,
        SyncError::Transport => BackendError::Transport,
    }
}
fn map_submit_backend(e: SubmitError) -> BackendError {
    match e {
        SubmitError::Store => BackendError::Store,
        SubmitError::Authentication => BackendError::Authentication,
        SubmitError::Cancelled => BackendError::Cancelled,
        SubmitError::Timeout => BackendError::Timeout,
        SubmitError::Transport => BackendError::Transport,
        SubmitError::OutboxOwned => BackendError::Capacity,
        SubmitError::RecipientRejected | SubmitError::Protocol | SubmitError::SizeLimit => {
            BackendError::Protocol
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::{Arc, Mutex};
    struct Script {
        input: std::io::Cursor<Vec<u8>>,
        output: Arc<Mutex<Vec<u8>>>,
    }
    impl ByteStream for Script {
        fn read(&mut self, b: &mut [u8]) -> Result<usize, TransportError> {
            self.input.read(b).map_err(|_| TransportError::Io)
        }
        fn write_all(&mut self, b: &[u8]) -> Result<(), TransportError> {
            self.output.lock().unwrap().extend_from_slice(b);
            Ok(())
        }
    }
    struct FakeTransport {
        script: Mutex<Option<Script>>,
        output: Arc<Mutex<Vec<u8>>>,
        service: i2pr_mail_domain::MailService,
    }

    struct QueueTransport(
        Mutex<std::collections::VecDeque<(i2pr_mail_domain::MailService, Script)>>,
    );
    impl MailTransport for QueueTransport {
        fn open(
            &self,
            service: i2pr_mail_domain::MailService,
            _control: OperationControl,
        ) -> Result<Box<dyn ByteStream>, TransportError> {
            let (expected, script) = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(TransportError::Unavailable)?;
            if expected != service {
                return Err(TransportError::Denied);
            }
            Ok(Box::new(script))
        }
    }
    struct TestCredentials;
    impl CredentialSource for TestCredentials {
        fn credentials(
            &self,
            _: &str,
            _: i2pr_mail_domain::MailService,
        ) -> Result<Credentials, BackendError> {
            Ok(Credentials::new("alice", "secret"))
        }
    }
    impl MailTransport for FakeTransport {
        fn open(
            &self,
            service: i2pr_mail_domain::MailService,
            _control: OperationControl,
        ) -> Result<Box<dyn ByteStream>, TransportError> {
            assert_eq!(service, self.service);
            Ok(Box::new(self.script.lock().unwrap().take().unwrap()))
        }
    }

    #[test]
    fn pop3_synthetic_server_syncs_header_by_opaque_uidl_and_keeps_auth_out_of_outputs() {
        let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 42\r\n+OK list\r\n1 ../uidl\r\n.\r\n+OK sizes\r\n1 42\r\n.\r\n+OK top\r\nSubject: hello\r\nFrom: a@i2p\r\n\r\n.\r\n+OK bye\r\n".to_vec();
        let output = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            script: Mutex::new(Some(Script {
                input: std::io::Cursor::new(transcript),
                output: output.clone(),
            })),
            output,
            service: i2pr_mail_domain::MailService::Pop3,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        let report = sync_pop3(&transport, &mut store, "account", "alice", "secret").unwrap();
        assert_eq!(report.remote_count, 1);
        assert_eq!(report.headers_cached, 1);
        let stored = store
            .message_by_uidl("account", "../uidl")
            .unwrap()
            .unwrap();
        assert_eq!(stored.receive_state, "HeaderCached");
        assert_eq!(
            store.raw_for_message(&stored.id).unwrap().unwrap(),
            b"Subject: hello\r\nFrom: a@i2p\r\n\r\n"
        );
        let sent = String::from_utf8(transport.output.lock().unwrap().clone()).unwrap();
        assert!(sent.contains("USER alice\r\n"));
        assert!(sent.contains("PASS secret\r\n"));
    }

    #[test]
    fn pop3_retr_fallback_when_top_is_rejected_caches_complete_entity() {
        let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 21\r\n+OK list\r\n1 uidl\r\n.\r\n+OK sizes\r\n1 21\r\n.\r\n-ERR TOP unsupported\r\n+OK retrieved\r\nSubject: fallback\r\n\r\nbody\r\n.\r\n+OK bye\r\n".to_vec();
        let output = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            script: Mutex::new(Some(Script {
                input: std::io::Cursor::new(transcript),
                output: output.clone(),
            })),
            output,
            service: i2pr_mail_domain::MailService::Pop3,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        sync_pop3(&transport, &mut store, "a", "u", "p").unwrap();
        let item = store.message_by_uidl("a", "uidl").unwrap().unwrap();
        assert_eq!(item.receive_state, "BodyCached");
        assert_eq!(
            store.raw_for_message(&item.id).unwrap().unwrap(),
            b"Subject: fallback\r\n\r\nbody\r\n"
        );
        assert!(
            String::from_utf8(transport.output.lock().unwrap().clone())
                .unwrap()
                .contains("RETR 1\r\n")
        );
    }

    #[test]
    fn pop3_delete_is_committed_only_after_quit_confirmation() {
        let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 4\r\n+OK list\r\n1 opaque\r\n.\r\n+OK sizes\r\n1 4\r\n.\r\n+OK deleted\r\n+OK bye\r\n".to_vec();
        let output = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            script: Mutex::new(Some(Script {
                input: std::io::Cursor::new(transcript),
                output: output.clone(),
            })),
            output,
            service: i2pr_mail_domain::MailService::Pop3,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        store
            .save_message(
                "local",
                "account",
                Some("opaque"),
                "HeaderCached",
                b"Subject: x\r\n\r\n",
            )
            .unwrap();
        store.mark_delete_pending("account", "opaque").unwrap();
        sync_pop3(&transport, &mut store, "account", "alice", "secret").unwrap();
        assert_eq!(
            store
                .message_by_uidl("account", "opaque")
                .unwrap()
                .unwrap()
                .receive_state,
            "RemoteDeletionCommitted"
        );
        let sent = String::from_utf8(transport.output.lock().unwrap().clone()).unwrap();
        assert!(sent.contains("DELE 1\r\n"));
        assert!(sent.contains("QUIT\r\n"));
    }

    #[test]
    fn pop3_missing_pending_uidl_is_reconciled_as_committed() {
        let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 0 0\r\n+OK list\r\n.\r\n+OK sizes\r\n.\r\n+OK bye\r\n".to_vec();
        let output = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            script: Mutex::new(Some(Script {
                input: std::io::Cursor::new(transcript),
                output: output.clone(),
            })),
            output,
            service: i2pr_mail_domain::MailService::Pop3,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        store
            .save_message(
                "local",
                "account",
                Some("gone"),
                "HeaderCached",
                b"Subject: x\r\n\r\n",
            )
            .unwrap();
        store.mark_delete_pending("account", "gone").unwrap();
        sync_pop3(&transport, &mut store, "account", "alice", "secret").unwrap();
        assert_eq!(
            store
                .message_by_uidl("account", "gone")
                .unwrap()
                .unwrap()
                .receive_state,
            "RemoteDeletionCommitted"
        );
    }

    #[test]
    fn pop3_failed_quit_leaves_delete_reconcilable() {
        let transcript=b"+OK ready\r\n-ERR no cap\r\n+OK user\r\n+OK pass\r\n+OK 1 4\r\n+OK list\r\n1 opaque\r\n.\r\n+OK sizes\r\n1 4\r\n.\r\n+OK deleted\r\n-ERR quit failed\r\n".to_vec();
        let output = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            script: Mutex::new(Some(Script {
                input: std::io::Cursor::new(transcript),
                output: output.clone(),
            })),
            output,
            service: i2pr_mail_domain::MailService::Pop3,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        store
            .save_message(
                "local",
                "account",
                Some("opaque"),
                "HeaderCached",
                b"Subject: x\r\n\r\n",
            )
            .unwrap();
        store.mark_delete_pending("account", "opaque").unwrap();
        assert_eq!(
            sync_pop3(&transport, &mut store, "account", "alice", "secret"),
            Err(SyncError::Protocol)
        );
        assert_eq!(
            store
                .message_by_uidl("account", "opaque")
                .unwrap()
                .unwrap()
                .receive_state,
            "DeleteMarkedSession"
        );
    }

    #[test]
    fn pop3_on_demand_body_fetch_preserves_complete_entity() {
        let transcript=b"+OK ready\r\n+OK user\r\n+OK pass\r\n+OK list\r\n1 opaque\r\n.\r\n+OK retrieved\r\nSubject: hello\r\n\r\nfull body\r\n.\r\n+OK bye\r\n".to_vec();
        let output = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            script: Mutex::new(Some(Script {
                input: std::io::Cursor::new(transcript),
                output: output.clone(),
            })),
            output,
            service: i2pr_mail_domain::MailService::Pop3,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        store
            .save_message(
                "local",
                "account",
                Some("opaque"),
                "HeaderCached",
                b"Subject: hello\r\n\r\n",
            )
            .unwrap();
        fetch_pop3_body(
            &transport, &mut store, "account", "alice", "secret", "opaque",
        )
        .unwrap();
        let item = store.message_by_uidl("account", "opaque").unwrap().unwrap();
        assert_eq!(item.receive_state, "BodyCached");
        assert_eq!(
            store.raw_for_message(&item.id).unwrap().unwrap(),
            b"Subject: hello\r\n\r\nfull body\r\n"
        );
    }

    struct ControlledFailureTransport(TransportError);
    struct ControlledFailureStream {
        error: TransportError,
    }
    impl ByteStream for ControlledFailureStream {
        fn read(&mut self, _: &mut [u8]) -> Result<usize, TransportError> {
            Err(self.error)
        }
        fn write_all(&mut self, _: &[u8]) -> Result<(), TransportError> {
            Err(self.error)
        }
    }
    impl MailTransport for ControlledFailureTransport {
        fn open(
            &self,
            _: i2pr_mail_domain::MailService,
            _: OperationControl,
        ) -> Result<Box<dyn ByteStream>, TransportError> {
            Ok(Box::new(ControlledFailureStream { error: self.0 }))
        }
    }

    #[test]
    fn transport_cancellation_and_timeout_map_to_typed_runtime_results() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        let control = OperationControl::new(std::time::Duration::from_secs(60));
        assert_eq!(
            sync_pop3_with_control(
                &ControlledFailureTransport(TransportError::Cancelled),
                &mut store,
                "a",
                "u",
                "p",
                control
            ),
            Err(SyncError::Cancelled)
        );
        let control = OperationControl::new(std::time::Duration::from_secs(60));
        assert_eq!(
            sync_pop3_with_control(
                &ControlledFailureTransport(TransportError::Timeout),
                &mut store,
                "a",
                "u",
                "p",
                control
            ),
            Err(SyncError::Timeout)
        );
    }

    #[test]
    fn cancelled_operation_fails_before_transport_open() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let transport = FakeTransport {
            script: Mutex::new(None),
            output,
            service: i2pr_mail_domain::MailService::Pop3,
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
        let control = OperationControl::new(std::time::Duration::from_secs(60));
        control.cancel();
        assert_eq!(
            sync_pop3_with_control(
                &transport, &mut store, "account", "alice", "secret", control
            ),
            Err(SyncError::Cancelled)
        );
    }

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
            Some(("Sent".into(), 2))
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
        let sent = String::from_utf8(transport.output.lock().unwrap().clone()).unwrap();
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
            Some(("DeliveryUnknown".into(), 1))
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
            Some(("FailedSafeToRetry".into(), 0))
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
            Some(("FailedSafeToRetry".into(), 0))
        );
        assert!(
            !String::from_utf8(output.lock().unwrap().clone())
                .unwrap()
                .contains("DATA\r\n")
        );
    }

    #[test]
    fn backend_service_runs_fake_mail_workflow_restarts_offline_and_bounds_handles() {
        let pop3=b"+OK ready\r\n+OK capabilities\r\nTOP\r\nUIDL\r\n.\r\n+OK user\r\n+OK pass\r\n+OK 1 22\r\n+OK uidls\r\n1 opaque\r\n.\r\n+OK list\r\n1 22\r\n.\r\n+OK top\r\nSubject: received\r\n\r\n.\r\n+OK bye\r\n".to_vec();
        let smtp=b"220 ready\r\n250-postman\r\n250-SIZE 100000\r\n250-AUTH LOGIN\r\n250 done\r\n334 user\r\n334 pass\r\n235 auth\r\n250 sender\r\n250 recipient\r\n354 data\r\n250 accepted\r\n221 bye\r\n".to_vec();
        let scripts = std::collections::VecDeque::from([
            (
                i2pr_mail_domain::MailService::Pop3,
                Script {
                    input: std::io::Cursor::new(pop3),
                    output: Arc::new(Mutex::new(Vec::new())),
                },
            ),
            (
                i2pr_mail_domain::MailService::Smtp,
                Script {
                    input: std::io::Cursor::new(smtp),
                    output: Arc::new(Mutex::new(Vec::new())),
                },
            ),
        ]);
        let transport = Arc::new(QueueTransport(Mutex::new(scripts)));
        let dir = tempfile::tempdir().unwrap();
        let mut service = BackendService::open(dir.path(), "account", transport).unwrap();
        let sync = service.sync(
            RequestId::new("req-sync").unwrap(),
            &TestCredentials,
            OperationControl::new(std::time::Duration::from_secs(60)),
        );
        assert_eq!(sync.value.unwrap().headers_cached, 1);
        assert_eq!(sync.events.len(), 2);
        let listed = service
            .list_messages(RequestId::new("req-list").unwrap())
            .value
            .unwrap();
        let incoming = listed
            .iter()
            .find(|m| m.uidl.as_deref() == Some("opaque"))
            .unwrap();
        let handle = service
            .open_message(RequestId::new("req-open").unwrap(), &incoming.id)
            .value
            .unwrap();
        assert_eq!(
            service
                .read_content(RequestId::new("req-read").unwrap(), handle, 0, 100)
                .value
                .unwrap(),
            b"Subject: received\r\n\r\n"
        );
        service
            .save_draft(
                RequestId::new("req-draft").unwrap(),
                "draft-1",
                b"draft entity",
            )
            .value
            .unwrap();
        service
            .delete_draft(RequestId::new("req-draft-delete").unwrap(), "draft-1")
            .value
            .unwrap();
        let recipients = vec![("to@postman.i2p".to_owned(), "To".to_owned())];
        service
            .queue_send(
                RequestId::new("req-queue").unwrap(),
                "out-1",
                "from@postman.i2p",
                &recipients,
                b"Subject: send\r\n\r\nbody",
            )
            .value
            .unwrap();
        service
            .send_queued(
                RequestId::new("req-send").unwrap(),
                "out-1",
                &TestCredentials,
                OperationControl::new(std::time::Duration::from_secs(60)),
            )
            .value
            .unwrap();
        assert_eq!(
            service
                .inspect_outbox(RequestId::new("req-outbox").unwrap(), "out-1")
                .value
                .unwrap()
                .state,
            "Sent"
        );
        assert!(
            service
                .health(RequestId::new("req-health").unwrap())
                .value
                .unwrap()
                .ready
        );
        assert_eq!(
            service.service_profile().pop3.destination(),
            "pop.postman.i2p"
        );
        let listed = service
            .list_messages(RequestId::new("req-list-2").unwrap())
            .value
            .unwrap();
        assert!(listed.iter().any(|m| m.state == "Sent"));
        let incoming_id = listed
            .iter()
            .find(|m| m.uidl.as_deref() == Some("opaque"))
            .unwrap()
            .id
            .clone();
        service
            .delete_local(RequestId::new("req-local-delete").unwrap(), &incoming_id)
            .value
            .unwrap();
        let after_delete = service
            .list_messages(RequestId::new("req-list-deleted").unwrap())
            .value
            .unwrap();
        assert!(
            after_delete
                .iter()
                .find(|m| m.id == incoming_id)
                .unwrap()
                .locally_deleted
        );
        assert_eq!(
            service
                .list_messages(RequestId::new("req-list").unwrap())
                .value,
            Err(BackendError::InvalidRequest)
        );
        assert_eq!(
            service
                .read_content(
                    RequestId::new("req-too-large").unwrap(),
                    handle,
                    0,
                    MAX_CONTENT_READ + 1
                )
                .value,
            Err(BackendError::Capacity)
        );
        service
            .release_content(RequestId::new("req-close").unwrap(), handle)
            .value
            .unwrap();
        assert_eq!(
            service
                .read_content(RequestId::new("req-stale-handle").unwrap(), handle, 0, 10)
                .value,
            Err(BackendError::NotFound)
        );
        service.shutdown();

        {
            let mut store = i2pr_mail_store::Store::open(dir.path()).unwrap();
            let recipients = vec![("pending@postman.i2p".to_owned(), "To".to_owned())];
            store
                .queue_outbox_full(
                    "out-unknown",
                    "from@postman.i2p",
                    b"uncertain entity",
                    &recipients,
                )
                .unwrap();
            store.claim_outbox("out-unknown").unwrap();
            store
                .set_outbox_stage("out-unknown", 1, "Submitting")
                .unwrap();
        }
        let offline = Arc::new(QueueTransport(Mutex::new(Default::default())));
        let mut restarted = BackendService::open(dir.path(), "account", offline).unwrap();
        let listed = restarted
            .list_messages(RequestId::new("req-offline").unwrap())
            .value
            .unwrap();
        assert!(listed.iter().any(|m| m.state == "Sent"));
        assert_eq!(
            restarted
                .inspect_outbox(RequestId::new("req-unknown").unwrap(), "out-unknown")
                .value
                .unwrap()
                .state,
            "DeliveryUnknown"
        );
        assert_eq!(
            restarted
                .send_queued(
                    RequestId::new("req-no-retry").unwrap(),
                    "out-unknown",
                    &TestCredentials,
                    OperationControl::new(std::time::Duration::from_secs(60))
                )
                .value,
            Err(BackendError::DeliveryUnknown)
        );
        restarted
            .resolve_delivery_unknown(
                RequestId::new("req-resolve-unknown").unwrap(),
                "out-unknown",
                DeliveryResolution::PermitRetry,
            )
            .value
            .unwrap();
        assert_eq!(
            restarted
                .inspect_outbox(
                    RequestId::new("req-resolved-status").unwrap(),
                    "out-unknown"
                )
                .value
                .unwrap()
                .state,
            "FailedSafeToRetry"
        );
        let received = listed
            .iter()
            .find(|m| m.uidl.as_deref() == Some("opaque"))
            .unwrap();
        assert!(received.locally_deleted);
        let inbox_handle = restarted
            .open_message(
                RequestId::new("req-open-inbox-offline").unwrap(),
                &received.id,
            )
            .value
            .unwrap();
        assert_eq!(
            restarted
                .read_content(
                    RequestId::new("req-read-inbox-offline").unwrap(),
                    inbox_handle,
                    0,
                    100
                )
                .value
                .unwrap(),
            b"Subject: received\r\n\r\n"
        );
        let sent = listed.iter().find(|m| m.state == "Sent").unwrap();
        let handle = restarted
            .open_message(RequestId::new("req-open-sent").unwrap(), &sent.id)
            .value
            .unwrap();
        assert_eq!(
            restarted
                .read_content(RequestId::new("req-read-sent").unwrap(), handle, 0, 100)
                .value
                .unwrap(),
            b"Subject: send\r\n\r\nbody"
        );
    }
}
