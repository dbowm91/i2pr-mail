//! Deterministic, sans-I/O SAM 3.1 client protocol primitives.
//!
//! This crate is a pure codec. It performs no networking, no filesystem access,
//! no async work, no clock reads and no allocation beyond bounded `String` and
//! `Vec` values. It owns no router handle and no session handle: a transport
//! adapter reads bytes, hands them to this codec, and acts on the typed values
//! returned here. That split is deliberate — SAM state transitions must be
//! testable without a router, and the router connection itself is blocked on
//! upstream work that does not affect the wire format.
//!
//! The crate has zero dependencies, project or external, so it can be lifted
//! into an adapter crate without dragging a domain model along with it.
//!
//! # One deliberate protocol ambiguity
//!
//! A successful `STREAM CONNECT` has two documented shapes. The SAM v3
//! specification at <https://i2p.net/en/docs/api/samv3/> specifies
//! `SAM <session>:<stream> OK`, while the router implementation this client
//! will actually talk to emits `STREAM STATUS RESULT=OK`. Both are accepted by
//! [`reply::parse_reply`] and normalized to the single typed value
//! [`reply::SamReplyKind::StreamStatus`], so no caller ever has to branch on
//! which router answered. Normalizing is defensive rather than accommodating:
//! a client that only understood the specification spelling would fail every
//! connection against a real router.
//!
//! # Security posture
//!
//! Every value that reaches a formatted line is validated first. Names and
//! session ids must be whitespace-free, quote-free and backslash-free ASCII, so
//! a caller cannot inject a second SAM token or split a line. Destinations must
//! use the I2P base64 alphabet, so a standard-base64 `+` or `/` is rejected here
//! rather than being forwarded to a router that will not accept it. An
//! over-budget value is rejected, never truncated. Nothing in this crate logs,
//! and no error variant carries a supplied value.
//!
//! # Module layout
//!
//! * [`version`] — `major.minor` parsing and the closed version negotiation.
//! * [`reply`] — reply tokenizing, bounds enforcement and typed reads.
//! * [`client`] — command builders and the client-side ordering state machine.

pub mod client;
pub mod reply;
pub mod version;
