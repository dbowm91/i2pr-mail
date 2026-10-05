//! Reusable synthetic stream and transport fixtures.
//!
//! These are test-only stand-ins for the one authorized transport seam. They keep
//! scripted server transcripts so protocol orchestration can be driven without any
//! host networking.
#![allow(dead_code)]

use crate::{
    BackendError, ByteStream, CredentialSource, Credentials, MailTransport, OperationControl,
    TransportError,
};

use std::io::Read;
use std::sync::{Arc, Mutex};
pub(super) struct Script {
    pub input: std::io::Cursor<Vec<u8>>,
    pub output: Arc<Mutex<Vec<u8>>>,
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

pub(super) struct FakeTransport {
    pub script: Mutex<Option<Script>>,
    pub output: Arc<Mutex<Vec<u8>>>,
    pub service: i2pr_mail_domain::MailService,
}

pub(super) struct QueueTransport(
    pub Mutex<std::collections::VecDeque<(i2pr_mail_domain::MailService, Script)>>,
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

pub(super) struct TestCredentials;

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

impl FakeTransport {
    /// Everything this transport has written so far, for command assertions.
    pub(super) fn written(&self) -> Vec<u8> {
        self.output.lock().unwrap().clone()
    }
}

pub(super) struct ControlledFailureTransport(pub TransportError);

pub(super) struct ControlledFailureStream {
    pub error: TransportError,
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

pub(super) struct FailingCredentials;

impl CredentialSource for FailingCredentials {
    fn credentials(
        &self,
        _: &str,
        _: i2pr_mail_domain::MailService,
    ) -> Result<Credentials, BackendError> {
        Err(BackendError::Authentication)
    }
}

pub(super) fn pop3_script(
    transcript: &[u8],
    output: Arc<Mutex<Vec<u8>>>,
) -> (i2pr_mail_domain::MailService, Script) {
    (
        i2pr_mail_domain::MailService::Pop3,
        Script {
            input: std::io::Cursor::new(transcript.to_vec()),
            output,
        },
    )
}

pub(super) fn smtp_script(
    transcript: &[u8],
    output: Arc<Mutex<Vec<u8>>>,
) -> (i2pr_mail_domain::MailService, Script) {
    (
        i2pr_mail_domain::MailService::Smtp,
        Script {
            input: std::io::Cursor::new(transcript.to_vec()),
            output,
        },
    )
}
