//! Runtime owns authorized transport and orchestration; protocol crates stay sans-I/O.
pub trait MailTransport: Send + Sync {
    fn open(
        &self,
        service: i2pr_mail_domain::MailService,
    ) -> Result<Box<dyn ByteStream>, TransportError>;
}
pub trait ByteStream: Send {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), TransportError>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    Unavailable,
    Denied,
    Closed,
    Io,
}
