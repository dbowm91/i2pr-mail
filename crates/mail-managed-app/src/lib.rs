//! Application side of the i2pr managed-app v1 wire contract.
//!
//! This crate implements only the language-neutral application-role client and
//! bounded multiplexer described by M012. It speaks to an injected duplex byte
//! stream (production stdin/stdout composition belongs to M013/M006) and
//! exposes authorized logical `sam` streams as `AsyncRead + AsyncWrite` values.
//!
//! The wire contract frozen here is:
//!
//! * handshake: `I2PA` magic, major 1, minor 0, role Application (1), zero
//!   reserved byte — 8 bytes total;
//! * frame envelope: version 1, kind 1 (control) / 2 (data), zero flags,
//!   stream id (`0` for control, nonzero for data), 32-bit big-endian declared
//!   payload length — 12 bytes total, payload `<= 65_536`, control JSON
//!   `<= 16_384`;
//! * control vocabulary: app `hello`/`open`/`close`/`reset`, host `reply`/
//!   `capabilities`/`stream_closed`/`stream_reset`/`health`, each as a single
//!   flat JSON object with duplicate-key and unknown-field rejection.
//!
//! Every bound below is a named constant. No unbounded channel constructor is
//! used anywhere in this crate. Tokio machinery is confined to this adapter
//! crate; no lower mail crate gains an async dependency.
//!
//! # Security posture
//!
//! Application-requested capabilities are never authority. Only the host-issued
//! one-shot `capabilities` set is effective. `sam` open is refused locally
//! before id allocation when that grant is absent. Unknown, duplicate, stale,
//! wrong-direction and malformed inputs fail closed. Channel EOF or a fatal
//! framing error terminates the whole channel generation and wakes every
//! waiter; one logical stream close never destroys healthy siblings. No error
//! or debug output carries data payload bytes, credentials, or authority
//! values beyond bounded ids and static reason strings.

pub mod client;
pub mod control;
pub mod frame;
pub mod handshake;
pub mod launch;

pub use client::{AppService, ManagedAppClient, ManagedLogicalStream};
pub use control::{AppRequest, HostEvent};
pub use frame::{
    CHANNEL_MAX_QUEUED_BYTES, FRAME_HEADER_LEN, FRAME_VERSION, KIND_CONTROL, KIND_DATA,
    MAX_CONTROL_JSON_LEN, MAX_FRAME_PAYLOAD_LEN, MAX_LIVE_STREAMS, MAX_PENDING_REQUESTS,
    PER_STREAM_MAX_CHUNKS, PER_STREAM_MAX_QUEUED_BYTES, WRITER_QUEUE_CAP,
};
pub use handshake::{
    HANDSHAKE_LEN, HANDSHAKE_MAGIC, HANDSHAKE_MAJOR, HANDSHAKE_MINOR, ROLE_APPLICATION,
};
pub use launch::{ManagedLaunchIdentity, parse_launch_args};
