//! Bounded managed-app v1 application client and multiplexer.
//!
//! Ownership: exactly one reader task parses all inbound frames and exactly
//! one writer task serializes all outbound bytes. Logical streams never touch
//! the shared channel; they exchange opaque byte chunks through bounded
//! queues. Channel EOF or any fatal framing/control violation terminates the
//! whole generation and wakes every waiter. One stream close never destroys
//! healthy siblings.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::control::{AppRequest, HostEvent};
use crate::frame::{
    CHANNEL_MAX_QUEUED_BYTES, FRAME_HEADER_LEN, MAX_CONTROL_JSON_LEN, MAX_FRAME_PAYLOAD_LEN,
    MAX_LIVE_STREAMS, MAX_PENDING_REQUESTS, PER_STREAM_MAX_CHUNKS, PER_STREAM_MAX_QUEUED_BYTES,
    WRITER_QUEUE_CAP, decode_header, encode_header,
};
use crate::handshake::encode_handshake;
use crate::launch::{ManagedLaunchIdentity, validate_app_id, validate_instance_decimal};

/// Service selector for `open_service`. Only `sam` is usable in M012; the
/// frozen v1 names are representable without becoming usable through this
/// type because any other service must go through a rejected control path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppService {
    /// Managed-app `sam` service.
    Sam,
}

impl AppService {
    fn wire_name(self) -> &'static str {
        match self {
            AppService::Sam => "sam",
        }
    }
}

/// Typed client failures. Variants carry bounded ids and static reasons only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagedError {
    /// Handshake bytes were rejected.
    Handshake,
    /// Frame envelope was rejected.
    Frame,
    /// Control payload was rejected.
    Control,
    /// Hello reply was missing, uncorrelated, or reported failure.
    HelloFailed,
    /// Capabilities event was missing, duplicated, or malformed.
    CapabilitiesFailed,
    /// Effective capability set lacks the requested service.
    Denied,
    /// Too many pending requests (live ceiling reached).
    RequestSaturated,
    /// Too many live streams (live ceiling reached).
    StreamSaturated,
    /// Monotonic id space exhausted without wraparound.
    IdExhausted,
    /// Channel generation is terminal (EOF or fatal framing error).
    ChannelClosed,
    /// Logical stream was reset by the host.
    StreamReset,
    /// Logical stream is closed.
    StreamClosed,
    /// Underlying transport I/O failed.
    Io,
    /// Operation was cancelled via shutdown or future drop.
    Cancelled,
}

impl std::fmt::Display for ManagedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagedError::Handshake => write!(f, "managed-app handshake rejected"),
            ManagedError::Frame => write!(f, "managed-app frame rejected"),
            ManagedError::Control => write!(f, "managed-app control rejected"),
            ManagedError::HelloFailed => write!(f, "managed-app hello failed"),
            ManagedError::CapabilitiesFailed => {
                write!(f, "managed-app capabilities failed")
            }
            ManagedError::Denied => write!(f, "managed-app service denied"),
            ManagedError::RequestSaturated => {
                write!(f, "managed-app request ledger saturated")
            }
            ManagedError::StreamSaturated => {
                write!(f, "managed-app stream ledger saturated")
            }
            ManagedError::IdExhausted => write!(f, "managed-app id space exhausted"),
            ManagedError::ChannelClosed => write!(f, "managed-app channel closed"),
            ManagedError::StreamReset => write!(f, "managed-app stream reset"),
            ManagedError::StreamClosed => write!(f, "managed-app stream closed"),
            ManagedError::Io => write!(f, "managed-app transport io failed"),
            ManagedError::Cancelled => write!(f, "managed-app operation cancelled"),
        }
    }
}

impl std::error::Error for ManagedError {}

/// Monotonic nonzero id allocator with deterministic exhaustion.
///
/// Wraparound aliasing is forbidden: once the space is exhausted the
/// allocator returns `IdExhausted` instead of reusing an id.
#[derive(Debug)]
pub struct IdAllocator {
    next: u64,
    max: u64,
}

impl IdAllocator {
    /// Create an allocator starting at `start` (must be nonzero) up to `max`.
    pub fn new(start: u64, max: u64) -> Self {
        debug_assert!(start != 0);
        debug_assert!(max >= start);
        Self { next: start, max }
    }

    /// Allocate the next id or report exhaustion without wrapping.
    pub fn allocate(&mut self) -> Result<u64, ManagedError> {
        if self.next == 0 || self.next > self.max {
            return Err(ManagedError::IdExhausted);
        }
        let id = self.next;
        if id == self.max {
            self.next = self.max.wrapping_add(1);
        } else {
            self.next += 1;
        }
        Ok(id)
    }
}

struct StreamSlot {
    tx: Option<mpsc::Sender<Vec<u8>>>,
    queued_bytes: usize,
    remote_closed: bool,
    remote_reset: bool,
}

struct Inner {
    writer_tx: mpsc::Sender<Vec<u8>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<(), ManagedError>>>>,
    streams: Mutex<HashMap<u32, StreamSlot>>,
    capabilities: std::sync::RwLock<Option<HashSet<String>>>,
    health: std::sync::RwLock<Option<String>>,
    next_request: Mutex<IdAllocator>,
    next_stream: Mutex<IdAllocator>,
    closed: AtomicBool,
    fatal: AtomicBool,
    aggregate_bytes: AtomicUsize,
    capacity_notify: Notify,
    shutdown_notify: Notify,
}

impl Inner {
    fn is_terminal(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn terminate(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.shutdown_notify.notify_waiters();
            self.capacity_notify.notify_waiters();
            let mut pending = self.pending.lock().expect("pending lock");
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err(ManagedError::ChannelClosed));
            }
            let mut streams = self.streams.lock().expect("streams lock");
            for (_, slot) in streams.iter_mut() {
                slot.tx.take();
            }
        }
    }

    fn mark_fatal(&self) {
        self.fatal.store(true, Ordering::Release);
        self.terminate();
    }
}

/// Bounded managed-app application client.
///
/// Constructed once per channel generation via [`ManagedAppClient::connect`]
/// over injected async I/O. Production stdin/stdout composition belongs to
/// M013/M006 and must remain a tiny obvious wrapper around this type.
pub struct ManagedAppClient {
    inner: Arc<Inner>,
    _reader: JoinHandle<()>,
    _writer: JoinHandle<()>,
}

impl ManagedAppClient {
    /// Establish a channel generation: write handshake, send one hello with
    /// the trusted launch identity, require a correlated successful reply,
    /// then require exactly one `capabilities` event and freeze it.
    pub async fn connect<T>(
        mut io: T,
        identity: &ManagedLaunchIdentity,
    ) -> Result<Self, ManagedError>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        validate_app_id(&identity.app_id).map_err(|_| ManagedError::HelloFailed)?;
        validate_instance_decimal(&identity.instance_decimal)
            .map_err(|_| ManagedError::HelloFailed)?;
        if identity
            .instance_decimal
            .parse::<u128>()
            .map_err(|_| ManagedError::HelloFailed)?
            != identity.instance_id
        {
            return Err(ManagedError::HelloFailed);
        }
        io.write_all(&encode_handshake())
            .await
            .map_err(|_| ManagedError::Io)?;
        io.flush().await.map_err(|_| ManagedError::Io)?;
        let hello = AppRequest::Hello {
            id: 1,
            app_id: identity.app_id.clone(),
            instance: identity.instance_decimal.clone(),
        };
        write_control(&mut io, &hello.encode())
            .await
            .map_err(|_| ManagedError::Io)?;
        let reply_bytes = read_control(&mut io).await?;
        let reply = HostEvent::decode(&reply_bytes).map_err(|_| ManagedError::Control)?;
        match reply {
            HostEvent::Reply {
                id: 1, ok: true, ..
            } => {}
            HostEvent::Reply { .. } => return Err(ManagedError::HelloFailed),
            _ => return Err(ManagedError::HelloFailed),
        }
        let caps_bytes = read_control(&mut io).await?;
        let caps = HostEvent::decode(&caps_bytes).map_err(|_| ManagedError::Control)?;
        let granted = match caps {
            HostEvent::Capabilities { granted } => granted,
            _ => return Err(ManagedError::CapabilitiesFailed),
        };
        let frozen: HashSet<String> = granted.into_iter().collect();
        let (read_half, write_half) = tokio::io::split(io);
        let (writer_tx, writer_rx) = mpsc::channel::<Vec<u8>>(WRITER_QUEUE_CAP);
        let inner = Arc::new(Inner {
            writer_tx,
            pending: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            capabilities: std::sync::RwLock::new(Some(frozen)),
            health: std::sync::RwLock::new(None),
            next_request: Mutex::new(IdAllocator::new(2, u64::MAX)),
            next_stream: Mutex::new(IdAllocator::new(1, u32::MAX as u64)),
            closed: AtomicBool::new(false),
            fatal: AtomicBool::new(false),
            aggregate_bytes: AtomicUsize::new(0),
            capacity_notify: Notify::new(),
            shutdown_notify: Notify::new(),
        });
        let reader_inner = Arc::clone(&inner);
        let reader = tokio::spawn(async move {
            reader_loop(reader_inner, read_half).await;
        });
        let writer_inner = Arc::clone(&inner);
        let writer = tokio::spawn(async move {
            writer_loop(writer_inner, write_half, writer_rx).await;
        });
        Ok(Self {
            inner,
            _reader: reader,
            _writer: writer,
        })
    }

    /// Effective capability snapshot frozen at establishment.
    pub fn capabilities(&self) -> Vec<String> {
        let guard = self.inner.capabilities.read().expect("caps lock");
        let mut out: Vec<String> = guard.clone().unwrap_or_default().into_iter().collect();
        out.sort();
        out
    }

    /// Last validated host health hint, if any.
    pub fn last_health(&self) -> Option<String> {
        self.inner.health.read().expect("health lock").clone()
    }

    /// Whether the channel generation is terminal.
    pub fn is_closed(&self) -> bool {
        self.inner.is_terminal()
    }

    /// Shut down the channel generation: stop new opens, wake every waiter,
    /// terminate every logical stream, drain bounded queues, cancel tasks.
    pub fn shutdown(&self) {
        self.inner.terminate();
        self._reader.abort();
        self._writer.abort();
    }

    /// Open an authorized logical service stream.
    ///
    /// The effective `sam` grant is checked before any id is allocated or any
    /// byte is emitted. Cancellation before remote success releases local
    /// ownership and sends a best-effort close for the reserved id.
    pub async fn open_service(
        &self,
        service: AppService,
    ) -> Result<ManagedLogicalStream, ManagedError> {
        if self.inner.is_terminal() {
            return Err(ManagedError::ChannelClosed);
        }
        {
            let caps = self.inner.capabilities.read().expect("caps lock");
            let granted = caps.as_ref().ok_or(ManagedError::ChannelClosed)?;
            if !granted.contains(service.wire_name()) {
                return Err(ManagedError::Denied);
            }
        }
        let request_id = {
            let pending_len = self.inner.pending.lock().expect("pending lock").len();
            if pending_len >= MAX_PENDING_REQUESTS {
                return Err(ManagedError::RequestSaturated);
            }
            self.inner
                .next_request
                .lock()
                .expect("req lock")
                .allocate()?
        };
        let stream_id_u64 = {
            let live = self.inner.streams.lock().expect("streams lock").len();
            if live >= MAX_LIVE_STREAMS {
                return Err(ManagedError::StreamSaturated);
            }
            self.inner
                .next_stream
                .lock()
                .expect("stream lock")
                .allocate()?
        };
        let stream_id = u32::try_from(stream_id_u64).map_err(|_| ManagedError::IdExhausted)?;
        if stream_id == 0 {
            return Err(ManagedError::IdExhausted);
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let (stream_tx, stream_rx) = mpsc::channel::<Vec<u8>>(PER_STREAM_MAX_CHUNKS);
        {
            let mut pending = self.inner.pending.lock().expect("pending lock");
            if pending.contains_key(&request_id) {
                return Err(ManagedError::ChannelClosed);
            }
            pending.insert(request_id, reply_tx);
        }
        {
            let mut streams = self.inner.streams.lock().expect("streams lock");
            if streams.contains_key(&stream_id) {
                let mut pending = self.inner.pending.lock().expect("pending lock");
                pending.remove(&request_id);
                return Err(ManagedError::ChannelClosed);
            }
            streams.insert(
                stream_id,
                StreamSlot {
                    tx: Some(stream_tx),
                    queued_bytes: 0,
                    remote_closed: false,
                    remote_reset: false,
                },
            );
        }
        struct OpenGuard {
            inner: Weak<Inner>,
            request_id: u64,
            stream_id: u32,
            armed: bool,
        }
        impl Drop for OpenGuard {
            fn drop(&mut self) {
                if self.armed {
                    if let Some(inner) = self.inner.upgrade() {
                        {
                            let mut pending = inner.pending.lock().expect("pending lock");
                            pending.remove(&self.request_id);
                        }
                        let tx = {
                            let mut streams = inner.streams.lock().expect("streams lock");
                            streams.remove(&self.stream_id).and_then(|s| s.tx)
                        };
                        drop(tx);
                        let close = AppRequest::Close {
                            stream_id: self.stream_id,
                        };
                        let payload = close.encode();
                        if let Some(header) =
                            encode_header(crate::frame::KIND_CONTROL, 0, payload.len())
                        {
                            let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
                            frame.extend_from_slice(&header);
                            frame.extend_from_slice(&payload);
                            let _ = inner.writer_tx.try_send(frame);
                        }
                    }
                }
            }
        }
        let mut guard = OpenGuard {
            inner: Arc::downgrade(&self.inner),
            request_id,
            stream_id,
            armed: true,
        };
        let open = AppRequest::Open {
            id: request_id,
            service: service.wire_name().to_string(),
            stream_id,
        };
        let payload = open.encode();
        let header =
            encode_header(crate::frame::KIND_CONTROL, 0, payload.len()).ok_or_else(|| {
                guard.armed = false;
                cleanup_open(&self.inner, request_id, stream_id);
                ManagedError::Control
            })?;
        let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&payload);
        let send_result = tokio::select! {
            biased;
            _ = self.inner.shutdown_notify.notified() => Err(ManagedError::ChannelClosed),
            r = self.inner.writer_tx.send(frame) => r.map_err(|_| ManagedError::ChannelClosed),
        };
        if send_result.is_err() {
            guard.armed = false;
            cleanup_open(&self.inner, request_id, stream_id);
            return Err(ManagedError::ChannelClosed);
        }
        let outcome = tokio::select! {
            biased;
            r = reply_rx => r.map_err(|_| {
                guard.armed = false;
                cleanup_open(&self.inner, request_id, stream_id);
                ManagedError::ChannelClosed
            }),
            _ = self.inner.shutdown_notify.notified() => {
                guard.armed = false;
                cleanup_open(&self.inner, request_id, stream_id);
                Err(ManagedError::ChannelClosed)
            }
        }?;
        match outcome {
            Ok(()) => {
                guard.armed = false;
                Ok(ManagedLogicalStream::new(
                    stream_id,
                    Arc::downgrade(&self.inner),
                    stream_rx,
                ))
            }
            Err(e) => {
                guard.armed = false;
                cleanup_open(&self.inner, request_id, stream_id);
                Err(e)
            }
        }
    }
}

impl Drop for ManagedAppClient {
    fn drop(&mut self) {
        self.inner.terminate();
        self._reader.abort();
        self._writer.abort();
    }
}

fn cleanup_open(inner: &Arc<Inner>, request_id: u64, stream_id: u32) {
    {
        let mut pending = inner.pending.lock().expect("pending lock");
        pending.remove(&request_id);
    }
    {
        let mut streams = inner.streams.lock().expect("streams lock");
        streams.remove(&stream_id);
    }
}

async fn write_control<T>(io: &mut T, payload: &[u8]) -> Result<(), ManagedError>
where
    T: AsyncWrite + Unpin,
{
    if payload.len() > MAX_CONTROL_JSON_LEN {
        return Err(ManagedError::Control);
    }
    let header =
        encode_header(crate::frame::KIND_CONTROL, 0, payload.len()).ok_or(ManagedError::Frame)?;
    io.write_all(&header).await.map_err(|_| ManagedError::Io)?;
    io.write_all(payload).await.map_err(|_| ManagedError::Io)?;
    io.flush().await.map_err(|_| ManagedError::Io)?;
    Ok(())
}

async fn read_control<T>(io: &mut T) -> Result<Vec<u8>, ManagedError>
where
    T: AsyncRead + Unpin,
{
    let mut header_bytes = [0u8; FRAME_HEADER_LEN];
    io.read_exact(&mut header_bytes)
        .await
        .map_err(|_| ManagedError::ChannelClosed)?;
    let header = decode_header(&header_bytes).map_err(|_| ManagedError::Frame)?;
    if header.kind != crate::frame::KIND_CONTROL || header.stream_id != 0 {
        return Err(ManagedError::Frame);
    }
    let mut payload = vec![0u8; header.payload_len];
    if !payload.is_empty() {
        io.read_exact(&mut payload)
            .await
            .map_err(|_| ManagedError::ChannelClosed)?;
    }
    Ok(payload)
}

async fn writer_loop<W>(inner: Arc<Inner>, mut write: W, mut rx: mpsc::Receiver<Vec<u8>>)
where
    W: AsyncWrite + Unpin + Send,
{
    loop {
        if inner.is_terminal() {
            return;
        }
        let frame = tokio::select! {
            biased;
            _ = inner.shutdown_notify.notified() => {
                return;
            }
            f = rx.recv() => f,
        };
        let frame = match frame {
            Some(f) => f,
            None => return,
        };
        if inner.is_terminal() {
            return;
        }
        if write.write_all(&frame).await.is_err() {
            inner.mark_fatal();
            return;
        }
        if write.flush().await.is_err() {
            inner.mark_fatal();
            return;
        }
        inner.capacity_notify.notify_waiters();
    }
}

async fn reader_loop<R>(inner: Arc<Inner>, mut read: R)
where
    R: AsyncRead + Unpin + Send,
{
    loop {
        if inner.is_terminal() {
            return;
        }
        let mut header_bytes = [0u8; FRAME_HEADER_LEN];
        if read.read_exact(&mut header_bytes).await.is_err() {
            inner.terminate();
            return;
        }
        let header = match decode_header(&header_bytes) {
            Ok(h) => h,
            Err(_) => {
                inner.mark_fatal();
                return;
            }
        };
        let mut payload = vec![0u8; header.payload_len];
        if !payload.is_empty() && read.read_exact(&mut payload).await.is_err() {
            inner.terminate();
            return;
        }
        if header.kind == crate::frame::KIND_CONTROL {
            let event = match HostEvent::decode(&payload) {
                Ok(e) => e,
                Err(_) => {
                    inner.mark_fatal();
                    return;
                }
            };
            match event {
                HostEvent::Reply { id, ok, .. } => {
                    let tx = {
                        let mut pending = inner.pending.lock().expect("pending lock");
                        pending.remove(&id)
                    };
                    match tx {
                        Some(tx) => {
                            let outcome: Result<(), ManagedError> = if ok {
                                Ok(())
                            } else {
                                Err(ManagedError::Denied)
                            };
                            let _ = tx.send(outcome);
                        }
                        None => {
                            // Stale or duplicate correlation: fatal.
                            inner.mark_fatal();
                            return;
                        }
                    }
                }
                HostEvent::Capabilities { .. } => {
                    // Second capabilities event: fatal contract error.
                    inner.mark_fatal();
                    return;
                }
                HostEvent::StreamClosed { stream_id } => {
                    let mut streams = inner.streams.lock().expect("streams lock");
                    match streams.get_mut(&stream_id) {
                        Some(slot) => {
                            slot.remote_closed = true;
                            slot.tx.take();
                        }
                        None => {
                            // Benign close confirmation for a locally-closed
                            // stream (close race): the entry was released by
                            // close_local before the host confirmation
                            // arrived. Ignore without disturbing siblings.
                        }
                    }
                }
                HostEvent::StreamReset { stream_id, .. } => {
                    let mut streams = inner.streams.lock().expect("streams lock");
                    match streams.get_mut(&stream_id) {
                        Some(slot) => {
                            slot.remote_reset = true;
                            slot.tx.take();
                        }
                        None => {
                            // Benign reset confirmation for a locally-closed
                            // stream; ignore like close above.
                        }
                    }
                }
                HostEvent::Health { status } => {
                    if status.is_empty() || status.len() > crate::control::MAX_DETAIL_LEN {
                        inner.mark_fatal();
                        return;
                    }
                    *inner.health.write().expect("health lock") = Some(status);
                }
            }
        } else {
            // Data frame: route only to a live, open stream.
            let stream_id = header.stream_id;
            // Enforce aggregate + per-stream byte ceilings with bounded wait.
            loop {
                if inner.is_terminal() {
                    return;
                }
                let (ok, ceiling_hit) = {
                    let streams = inner.streams.lock().expect("streams lock");
                    match streams.get(&stream_id) {
                        Some(slot) => {
                            if slot.remote_closed || slot.remote_reset || slot.tx.is_none() {
                                (false, false)
                            } else {
                                let aggregate = inner.aggregate_bytes.load(Ordering::Acquire);
                                let over_aggregate = aggregate.saturating_add(payload.len())
                                    > CHANNEL_MAX_QUEUED_BYTES;
                                let over_stream = slot.queued_bytes.saturating_add(payload.len())
                                    > PER_STREAM_MAX_QUEUED_BYTES;
                                if over_aggregate || over_stream {
                                    (false, true)
                                } else {
                                    (true, false)
                                }
                            }
                        }
                        None => (false, false),
                    }
                };
                if ok {
                    break;
                }
                if !ceiling_hit {
                    // Unknown stream or data after close/reset: fatal.
                    inner.mark_fatal();
                    return;
                }
                // Ceiling hit: bounded wait for consumers to drain.
                tokio::select! {
                    biased;
                    _ = inner.shutdown_notify.notified() => {
                        return;
                    }
                    _ = inner.capacity_notify.notified() => {},
                }
            }
            let tx = {
                let streams = inner.streams.lock().expect("streams lock");
                streams.get(&stream_id).and_then(|s| s.tx.clone())
            };
            match tx {
                Some(tx) => {
                    let payload_len = payload.len();
                    {
                        let mut streams = inner.streams.lock().expect("streams lock");
                        if let Some(slot) = streams.get_mut(&stream_id) {
                            slot.queued_bytes = slot.queued_bytes.saturating_add(payload_len);
                        }
                    }
                    inner
                        .aggregate_bytes
                        .fetch_add(payload_len, Ordering::AcqRel);
                    // Bounded per-stream queue: awaits consumer drain.
                    let send_result = tokio::select! {
                        biased;
                        _ = inner.shutdown_notify.notified() => {
                            return;
                        }
                        r = tx.send(payload) => r,
                    };
                    if send_result.is_err() {
                        // Consumer dropped without close_local accounting.
                        inner.aggregate_bytes.fetch_sub(
                            payload_len.min(inner.aggregate_bytes.load(Ordering::Acquire)),
                            Ordering::AcqRel,
                        );
                    }
                }
                None => {
                    inner.mark_fatal();
                    return;
                }
            }
        }
    }
}

// ---- Logical stream ----

/// Isolated logical service stream over one nonzero managed-app stream id.
///
/// Implements `AsyncRead + AsyncWrite + Unpin + Send`. Writes frame opaque
/// bytes as data for that id; reads consume only data routed to that id.
/// Drop sends close at most once when healthy and never blocks.
pub struct ManagedLogicalStream {
    stream_id: u32,
    inner: Weak<Inner>,
    rx: mpsc::Receiver<Vec<u8>>,
    read_buf: Vec<u8>,
    read_pos: usize,
    eof_observed: bool,
    reset_observed: bool,
    close_sent: bool,
    pending_frame: Option<Vec<u8>>,
    pending_wait: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl ManagedLogicalStream {
    fn new(stream_id: u32, inner: Weak<Inner>, rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            stream_id,
            inner,
            rx,
            read_buf: Vec::new(),
            read_pos: 0,
            eof_observed: false,
            reset_observed: false,
            close_sent: false,
            pending_frame: None,
            pending_wait: None,
        }
    }

    /// Logical stream id.
    pub fn stream_id(&self) -> u32 {
        self.stream_id
    }

    /// Best-effort close: emit one close frame when healthy, release local
    /// ownership regardless, never block.
    pub fn close_local(&mut self) {
        if self.close_sent {
            return;
        }
        self.close_sent = true;
        if let Some(inner) = self.inner.upgrade() {
            let healthy = !inner.is_terminal();
            {
                let mut streams = inner.streams.lock().expect("streams lock");
                if let Some(slot) = streams.get_mut(&self.stream_id) {
                    let held = slot
                        .queued_bytes
                        .min(inner.aggregate_bytes.load(Ordering::Acquire));
                    inner.aggregate_bytes.fetch_sub(held, Ordering::AcqRel);
                    slot.queued_bytes = 0;
                    slot.tx.take();
                }
                streams.remove(&self.stream_id);
            }
            if healthy {
                let close = AppRequest::Close {
                    stream_id: self.stream_id,
                };
                let payload = close.encode();
                if let Some(header) = encode_header(crate::frame::KIND_CONTROL, 0, payload.len()) {
                    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
                    frame.extend_from_slice(&header);
                    frame.extend_from_slice(&payload);
                    let _ = inner.writer_tx.try_send(frame);
                }
            }
            inner.capacity_notify.notify_waiters();
        }
    }

    fn poll_rx(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), ManagedError>> {
        if self.eof_observed {
            return Poll::Ready(Ok(()));
        }
        if self.reset_observed {
            return Poll::Ready(Err(ManagedError::StreamReset));
        }
        if self.read_pos < self.read_buf.len() {
            return Poll::Ready(Ok(()));
        }
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(chunk)) => {
                if let Some(inner) = self.inner.upgrade() {
                    let mut streams = inner.streams.lock().expect("streams lock");
                    if let Some(slot) = streams.get_mut(&self.stream_id) {
                        slot.queued_bytes = slot.queued_bytes.saturating_sub(chunk.len());
                    }
                    let held = chunk
                        .len()
                        .min(inner.aggregate_bytes.load(Ordering::Acquire));
                    inner.aggregate_bytes.fetch_sub(held, Ordering::AcqRel);
                    inner.capacity_notify.notify_waiters();
                }
                self.read_buf = chunk;
                self.read_pos = 0;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                if let Some(inner) = self.inner.upgrade() {
                    let (closed, reset, fatal) = {
                        let streams = inner.streams.lock().expect("streams lock");
                        match streams.get(&self.stream_id) {
                            Some(s) => (s.remote_closed, s.remote_reset, false),
                            None => (true, false, inner.fatal.load(Ordering::Acquire)),
                        }
                    };
                    if reset {
                        self.reset_observed = true;
                        return Poll::Ready(Err(ManagedError::StreamReset));
                    }
                    if fatal && !closed {
                        return Poll::Ready(Err(ManagedError::ChannelClosed));
                    }
                    let _ = closed;
                    self.eof_observed = true;
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Ready(Err(ManagedError::ChannelClosed))
                }
            }
            Poll::Pending => {
                if let Some(inner) = self.inner.upgrade() {
                    if inner.fatal.load(Ordering::Acquire) {
                        return Poll::Ready(Err(ManagedError::ChannelClosed));
                    }
                    if inner.is_terminal() {
                        // Terminal with no queued bytes: EOF if remotely
                        // closed, else closed error after drain.
                        if self.read_pos < self.read_buf.len() {
                            return Poll::Ready(Ok(()));
                        }
                        self.eof_observed = true;
                        return Poll::Ready(Ok(()));
                    }
                } else {
                    return Poll::Ready(Err(ManagedError::ChannelClosed));
                }
                Poll::Pending
            }
        }
    }
}

impl AsyncRead for ManagedLogicalStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if self.read_pos < self.read_buf.len() {
            let available = &self.read_buf[self.read_pos..];
            let n = available.len().min(buf.remaining());
            buf.put_slice(&available[..n]);
            self.read_pos += n;
            return Poll::Ready(Ok(()));
        }
        match self.poll_rx(cx) {
            Poll::Ready(Ok(())) => {
                if self.eof_observed && self.read_pos >= self.read_buf.len() {
                    return Poll::Ready(Ok(()));
                }
                if self.read_pos < self.read_buf.len() {
                    let available = &self.read_buf[self.read_pos..];
                    let n = available.len().min(buf.remaining());
                    buf.put_slice(&available[..n]);
                    self.read_pos += n;
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Ready(Ok(()))
                }
            }
            Poll::Ready(Err(ManagedError::StreamReset)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "managed-app stream reset",
            ))),
            Poll::Ready(Err(_)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "managed-app channel closed",
            ))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for ManagedLogicalStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let inner = match self.inner.upgrade() {
            Some(i) => i,
            None => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionAborted,
                    "managed-app channel closed",
                )));
            }
        };
        if inner.is_terminal() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "managed-app channel closed",
            )));
        }
        // A previous frame is parked waiting for writer capacity.
        if let Some(frame) = self.pending_frame.take() {
            let mut wait = self.pending_wait.take().expect("wait follows frame");
            match wait.as_mut().poll(cx) {
                Poll::Pending => {
                    let this = self.as_mut().get_mut();
                    this.pending_frame = Some(frame);
                    this.pending_wait = Some(wait);
                    return Poll::Pending;
                }
                Poll::Ready(()) => {
                    if inner.is_terminal() {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::ConnectionAborted,
                            "managed-app channel closed",
                        )));
                    }
                    let n = frame.len() - FRAME_HEADER_LEN;
                    match inner.writer_tx.try_send(frame) {
                        Ok(()) => return Poll::Ready(Ok(n)),
                        Err(mpsc::error::TrySendError::Full(f)) => {
                            let this = self.as_mut().get_mut();
                            this.pending_frame = Some(f);
                            let inner_clone = Arc::clone(&inner);
                            this.pending_wait = Some(Box::pin(async move {
                                inner_clone.capacity_notify.notified().await;
                            }));
                            return Poll::Pending;
                        }
                        Err(_) => {
                            return Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::ConnectionAborted,
                                "managed-app channel closed",
                            )));
                        }
                    }
                }
            }
        }
        let n = buf.len().min(MAX_FRAME_PAYLOAD_LEN);
        let header = match encode_header(crate::frame::KIND_DATA, self.stream_id, n) {
            Some(h) => h,
            None => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "managed-app frame rejected",
                )));
            }
        };
        let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + n);
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&buf[..n]);
        match inner.writer_tx.try_send(frame) {
            Ok(()) => Poll::Ready(Ok(n)),
            Err(mpsc::error::TrySendError::Full(f)) => {
                let this = self.as_mut().get_mut();
                this.pending_frame = Some(f);
                let inner_clone = Arc::clone(&inner);
                this.pending_wait = Some(Box::pin(async move {
                    inner_clone.capacity_notify.notified().await;
                }));
                Poll::Pending
            }
            Err(_) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "managed-app channel closed",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.close_local();
        Poll::Ready(Ok(()))
    }
}

impl Drop for ManagedLogicalStream {
    fn drop(&mut self) {
        self.close_local();
    }
}

#[cfg(test)]
mod id_tests {
    use super::*;

    #[test]
    fn allocator_is_monotonic_and_exhausts_without_wrap() {
        let mut alloc = IdAllocator::new(1, 3);
        assert_eq!(alloc.allocate(), Ok(1));
        assert_eq!(alloc.allocate(), Ok(2));
        assert_eq!(alloc.allocate(), Ok(3));
        assert_eq!(alloc.allocate(), Err(ManagedError::IdExhausted));
        assert_eq!(alloc.allocate(), Err(ManagedError::IdExhausted));
    }

    #[test]
    fn allocator_at_u64_max_exhausts_deterministically() {
        let mut alloc = IdAllocator::new(u64::MAX - 1, u64::MAX);
        assert_eq!(alloc.allocate(), Ok(u64::MAX - 1));
        assert_eq!(alloc.allocate(), Ok(u64::MAX));
        assert_eq!(alloc.allocate(), Err(ManagedError::IdExhausted));
    }
}
