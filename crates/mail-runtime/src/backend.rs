//! Backend service lifecycle, request ledger, and application API.
use crate::pop3::{SyncError, SyncReport, fetch_pop3_body_with_control, sync_pop3_with_control};
use crate::smtp::{SmtpSubmission, SubmitError, submit_smtp};
use crate::transport::{MailTransport, OperationControl};
use i2pr_mail_domain::{StoredMessageState, SubmissionProgress, SubmissionState};

pub const MAX_ACTIVE_CONTENT_HANDLES: usize = 32;
pub const MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CONTENT_READ: usize = 64 * 1024;
/// Ceiling on requests a caller may hold in flight at once. This is a genuinely
/// live resource ceiling, so exhausting it is the only request-history condition
/// that may report `Capacity`.
pub const MAX_ACTIVE_REQUESTS: usize = 256;
/// Size of the recent-completion deduplication window. Reaching it evicts the
/// oldest completions in a deterministic order rather than failing, because no
/// durable cross-session idempotency contract is claimed.
pub const MAX_RECENT_COMPLETED_REQUESTS: usize = 1024;

/// Bounded request-ID ledger. Ownership is split between requests that are
/// currently in flight and a fixed-size window of recently completed requests, so
/// a long-lived service cannot permanently exhaust request capacity.
#[derive(Debug, Default)]
pub(crate) struct RequestLedger {
    pub(crate) active: std::collections::HashSet<RequestId>,
    completed: std::collections::VecDeque<RequestId>,
    completed_ids: std::collections::HashSet<RequestId>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LedgerError {
    Duplicate,
    Capacity,
}

impl RequestLedger {
    pub(crate) fn begin(&mut self, id: &RequestId) -> Result<(), LedgerError> {
        if self.active.contains(id) || self.completed_ids.contains(id) {
            return Err(LedgerError::Duplicate);
        }
        if self.active.len() >= MAX_ACTIVE_REQUESTS {
            return Err(LedgerError::Capacity);
        }
        self.active.insert(id.clone());
        Ok(())
    }

    /// Releases active ownership and retains the id for deduplication. Must be
    /// called on every terminal path, including validation, credential, transport,
    /// store, cancellation, and timeout failures.
    pub(crate) fn end(&mut self, id: &RequestId) {
        self.active.remove(id);
        if self.completed_ids.insert(id.clone()) {
            self.completed.push_back(id.clone());
            while self.completed.len() > MAX_RECENT_COMPLETED_REQUESTS {
                if let Some(evicted) = self.completed.pop_front() {
                    self.completed_ids.remove(&evicted);
                }
            }
        }
    }

    pub(crate) fn active_len(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn completed_len(&self) -> usize {
        self.completed.len()
    }
}

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
    pub state: StoredMessageState,
    pub locally_deleted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendHealth {
    pub ready: bool,
    pub active_content_handles: usize,
    /// Requests currently in flight. Returns to zero after every terminal path.
    pub active_requests: usize,
    /// Request ids retained in the bounded recent-completion window.
    pub retained_request_ids: usize,
    pub schema_version: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutboxStatus {
    pub state: SubmissionState,
    pub progress: SubmissionProgress,
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
    requests: RequestLedger,
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
            requests: RequestLedger::default(),
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
        self.terminal(
            request_id.clone(),
            result,
            vec![
                start,
                BackendEvent {
                    request_id,
                    kind: final_kind,
                },
            ],
        )
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
                active_requests: self.requests.active_len(),
                retained_request_ids: self.requests.completed_len(),
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
                v.map(|(state, progress)| OutboxStatus { state, progress })
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
                            state: m.state,
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
            if state.0 == SubmissionState::DeliveryUnknown {
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
    /// Terminal path for every request: releases active ownership and enters the
    /// bounded recent-completion window, whatever the outcome was.
    fn terminal<T>(
        &mut self,
        request_id: RequestId,
        result: Result<T, BackendError>,
        events: Vec<BackendEvent>,
    ) -> OperationResult<T> {
        self.requests.end(&request_id);
        OperationResult {
            request_id,
            value: result,
            events,
        }
    }
    fn result<T>(
        &mut self,
        request_id: RequestId,
        result: Result<T, BackendError>,
    ) -> OperationResult<T> {
        let kind = match &result {
            Ok(_) => BackendEventKind::Completed,
            Err(e) => BackendEventKind::Failed(e.clone()),
        };
        self.terminal(
            request_id.clone(),
            result,
            vec![BackendEvent { request_id, kind }],
        )
    }
    fn begin_request(&mut self, id: &RequestId) -> Result<(), BackendError> {
        self.requests.begin(id).map_err(|error| match error {
            LedgerError::Duplicate => BackendError::InvalidRequest,
            LedgerError::Capacity => BackendError::Capacity,
        })
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
