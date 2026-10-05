//! The single authorized transport seam and its bounded line plumbing.
//!
//! `MailTransport` is the only remote I/O path in the backend. It opens a logical
//! `MailService` stream and nothing else: protocol code can never name a host or a
//! port. Bounded line reading lives here too, but the line ceiling belongs to the
//! protocol, so a failure is reported as a neutral `LineError` that each protocol
//! driver maps into its own typed error.
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
    /// Reports cancellation or deadline expiry before an operation proceeds. The
    /// result is a transport-level error; protocol drivers map it into their own
    /// vocabulary at the call site.
    pub fn check(&self) -> Result<(), TransportError> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            Err(TransportError::Cancelled)
        } else if std::time::Instant::now() >= self.deadline {
            Err(TransportError::Timeout)
        } else {
            Ok(())
        }
    }
}

/// Writes a verb plus CRLF.
pub(crate) fn write_command(stream: &mut dyn ByteStream, command: &[u8]) -> Result<(), LineError> {
    stream.write_all(command).map_err(map_line_transport)?;
    stream.write_all(b"\r\n").map_err(map_line_transport)
}

/// Writes a line the protocol crate already bounded and terminated, so no command
/// buffer is formatted at the call site.
pub(crate) fn write_line(stream: &mut dyn ByteStream, line: &[u8]) -> Result<(), LineError> {
    stream.write_all(line).map_err(map_line_transport)
}
fn map_line_transport(error: TransportError) -> LineError {
    match error {
        TransportError::Timeout => LineError::Timeout,
        TransportError::Cancelled => LineError::Cancelled,
        _ => LineError::Io,
    }
}

/// Reads one CRLF-terminated protocol line, refusing to grow past the protocol
/// line ceiling. Protocol drivers own this limit, not the transport.
pub(crate) fn read_line(stream: &mut dyn ByteStream, out: &mut Vec<u8>) -> Result<(), LineError> {
    out.clear();
    loop {
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => return Err(LineError::Closed),
            Ok(_) => {}
            Err(TransportError::Timeout) => return Err(LineError::Timeout),
            Err(TransportError::Cancelled) => return Err(LineError::Cancelled),
            Err(_) => return Err(LineError::Io),
        }
        out.push(byte[0]);
        if out.len() > i2pr_mail_proto::pop3::MAX_LINE {
            return Err(LineError::TooLong);
        }
        if out.ends_with(b"\r\n") {
            return Ok(());
        }
    }
}

/// Failure of a bounded line read, kept neutral so each protocol driver maps it
/// into its own typed error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineError {
    Timeout,
    Cancelled,
    Io,
    Closed,
    TooLong,
}
