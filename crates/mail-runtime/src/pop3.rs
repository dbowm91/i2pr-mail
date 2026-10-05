//! POP3 receive orchestration over the single authorized transport seam.
use crate::transport::{
    self, ByteStream, LineError, MailTransport, OperationControl, TransportError,
};
use i2pr_mail_domain::{ReceiveState, StoredMessageState};
use i2pr_mail_proto::pop3;

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

/// Transport failures become POP3-typed errors at this boundary.
fn map_sync_transport(error: TransportError) -> SyncError {
    match error {
        TransportError::Timeout => SyncError::Timeout,
        TransportError::Cancelled => SyncError::Cancelled,
        _ => SyncError::Transport,
    }
}

/// Transport-neutral line failures become POP3-typed errors at this boundary.
fn map_line_error(error: LineError) -> SyncError {
    match error {
        LineError::Timeout => SyncError::Timeout,
        LineError::Cancelled => SyncError::Cancelled,
        LineError::TooLong => SyncError::Limits,
        LineError::Io | LineError::Closed => SyncError::Transport,
    }
}

fn read_line(stream: &mut dyn ByteStream, out: &mut Vec<u8>) -> Result<(), SyncError> {
    transport::read_line(stream, out).map_err(map_line_error)
}

fn write_command(stream: &mut dyn ByteStream, command: &[u8]) -> Result<(), SyncError> {
    transport::write_command(stream, command).map_err(map_line_error)
}

/// Writes a line the protocol crate already bounded and terminated.
fn write_line(stream: &mut dyn ByteStream, line: &[u8]) -> Result<(), SyncError> {
    transport::write_line(stream, line).map_err(map_line_error)
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
    control.check().map_err(map_sync_transport)?;
    let mut stream = transport
        .open(i2pr_mail_domain::MailService::Pop3, control.clone())
        .map_err(map_sync_transport)?;
    expect_ok(stream.as_mut())?;
    write_command(stream.as_mut(), b"CAPA")?;
    if read_status(stream.as_mut())? {
        let _ = read_multiline(stream.as_mut())?;
    }
    // Credentials are bounded against the frozen POP3 ceilings before any command
    // buffer is formatted, and a rejected credential never reaches the stream.
    let user_line = pop3::user_command(username).map_err(|_| SyncError::Authentication)?;
    let pass_line = pop3::pass_command(password).map_err(|_| SyncError::Authentication)?;
    write_line(stream.as_mut(), &user_line)?;
    expect_ok(stream.as_mut())?;
    write_line(stream.as_mut(), &pass_line)?;
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
                .set_receive_state(account, &uidl, ReceiveState::RemoteDeletionCommitted)
                .map_err(|_| SyncError::Store)?;
        } else if state == ReceiveState::DeleteMarkedSession {
            store
                .set_receive_state(account, &uidl, ReceiveState::DeletePending)
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
            if existing.state == StoredMessageState::Receive(ReceiveState::DeletePending) {
                let dele_line =
                    pop3::dele_command(entry.ordinal).map_err(|_| SyncError::Protocol)?;
                write_line(stream.as_mut(), &dele_line)?;
                expect_ok(stream.as_mut())?;
                store
                    .set_receive_state(account, &entry.uidl, ReceiveState::DeleteMarkedSession)
                    .map_err(|_| SyncError::Store)?;
            }
            continue;
        }
        let top_line = pop3::top_command(entry.ordinal).map_err(|_| SyncError::Protocol)?;
        write_line(stream.as_mut(), &top_line)?;
        let (entity, complete) = if read_status(stream.as_mut())? {
            (unstuff(read_multiline(stream.as_mut())?), false)
        } else {
            let retr_line = pop3::retr_command(entry.ordinal).map_err(|_| SyncError::Protocol)?;
            write_line(stream.as_mut(), &retr_line)?;
            expect_ok(stream.as_mut())?;
            (unstuff(read_multiline(stream.as_mut())?), true)
        };
        let state = if complete {
            ReceiveState::BodyCached
        } else {
            ReceiveState::HeaderCached
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
        report.headers_cached += usize::from(state == ReceiveState::HeaderCached);
        report.bodies_cached += usize::from(state == ReceiveState::BodyCached);
    }
    write_command(stream.as_mut(), b"QUIT")?;
    expect_ok(stream.as_mut())?;
    for (uidl, state) in store
        .deletion_states(account)
        .map_err(|_| SyncError::Store)?
    {
        if state == ReceiveState::DeleteMarkedSession {
            store
                .set_receive_state(account, &uidl, ReceiveState::RemoteDeletionCommitted)
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
    control.check().map_err(map_sync_transport)?;
    let user_line = pop3::user_command(username).map_err(|_| SyncError::Authentication)?;
    let pass_line = pop3::pass_command(password).map_err(|_| SyncError::Authentication)?;
    let mut stream = transport
        .open(i2pr_mail_domain::MailService::Pop3, control.clone())
        .map_err(map_sync_transport)?;
    expect_ok(stream.as_mut())?;
    write_line(stream.as_mut(), &user_line)?;
    expect_ok(stream.as_mut())?;
    write_line(stream.as_mut(), &pass_line)?;
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
    let retr_line = pop3::retr_command(ordinal).map_err(|_| SyncError::Protocol)?;
    write_line(stream.as_mut(), &retr_line)?;
    expect_ok(stream.as_mut())?;
    let raw = unstuff(read_multiline(stream.as_mut())?);
    let existing = store
        .message_by_uidl(account, uidl)
        .map_err(|_| SyncError::Store)?
        .ok_or(SyncError::Store)?;
    store
        .save_message(
            &existing.id,
            account,
            Some(uidl),
            ReceiveState::BodyCached,
            &raw,
        )
        .map_err(|_| SyncError::Store)?;
    write_command(stream.as_mut(), b"QUIT")?;
    expect_ok(stream.as_mut())?;
    Ok(())
}
