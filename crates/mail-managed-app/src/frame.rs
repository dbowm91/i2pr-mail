//! Bounded managed-app v1 frame envelope codec.
//!
//! Header layout (12 bytes, network order for multi-byte fields):
//!
//! ```text
//! offset 0: version u8 == 1
//! offset 1: kind u8 == 1 (control) | 2 (data)
//! offset 2..4: flags u16 == 0
//! offset 4..8: stream id u32 BE (0 for control, nonzero for data)
//! offset 8..12: payload length u32 BE (declared length, <= 65_536)
//! ```
//!
//! Control payloads are UTF-8 JSON and additionally bounded to
//! `<= 16_384` bytes. Data payloads are opaque bytes. Every violation —
//! wrong version/kind/flags, impossible stream-id direction, oversized or
//! truncated payload — is a fatal channel error.

use thiserror::Error;

/// Frame protocol version.
pub const FRAME_VERSION: u8 = 1;
/// Control frame kind (stream id must be 0).
pub const KIND_CONTROL: u8 = 1;
/// Data frame kind (stream id must be nonzero).
pub const KIND_DATA: u8 = 2;
/// Frame header length in bytes.
pub const FRAME_HEADER_LEN: usize = 12;
/// Maximum frame payload in bytes.
pub const MAX_FRAME_PAYLOAD_LEN: usize = 65_536;
/// Maximum control JSON payload in bytes (stricter than the frame ceiling).
pub const MAX_CONTROL_JSON_LEN: usize = 16_384;
/// Maximum concurrently pending request ids.
pub const MAX_PENDING_REQUESTS: usize = 64;
/// Maximum concurrently live logical streams.
pub const MAX_LIVE_STREAMS: usize = 128;
/// Per-stream inbound chunk queue capacity (chunk count).
pub const PER_STREAM_MAX_CHUNKS: usize = 32;
/// Per-stream inbound queued-byte ceiling.
pub const PER_STREAM_MAX_QUEUED_BYTES: usize = 256 * 1024;
/// Channel-wide aggregate queued-byte ceiling across all live streams.
pub const CHANNEL_MAX_QUEUED_BYTES: usize = 1024 * 1024;
/// Serialized writer queue capacity (frame count).
pub const WRITER_QUEUE_CAP: usize = 64;

/// Frame decode failures. Variants carry only static reasons and bounded
/// lengths, never payload bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum FrameError {
    /// Header is shorter than 12 bytes.
    #[error("frame header truncated")]
    TruncatedHeader,
    /// Version byte is not 1.
    #[error("frame version mismatch")]
    Version,
    /// Kind byte is not 1 or 2.
    #[error("frame kind unknown")]
    Kind,
    /// Flags field is nonzero.
    #[error("frame flags nonzero")]
    Flags,
    /// Control frame on a nonzero stream id.
    #[error("control frame on nonzero stream")]
    ControlStreamId,
    /// Data frame on stream id 0.
    #[error("data frame on stream zero")]
    DataStreamId,
    /// Declared payload length exceeds the frame ceiling.
    #[error("frame payload oversize")]
    Oversize,
    /// Control payload exceeds the JSON ceiling.
    #[error("control payload oversize")]
    ControlOversize,
    /// Declared payload is longer than the bytes available.
    #[error("frame payload truncated")]
    TruncatedPayload,
}

/// A decoded frame header with no payload attached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    /// 1 for control, 2 for data.
    pub kind: u8,
    /// Stream id: 0 for control, nonzero for data.
    pub stream_id: u32,
    /// Declared payload length.
    pub payload_len: usize,
}

/// Encode a 12-byte header for the given kind/stream id/payload length.
///
/// Returns `None` when the request violates direction or size rules instead
/// of emitting a malformed header.
pub fn encode_header(
    kind: u8,
    stream_id: u32,
    payload_len: usize,
) -> Option<[u8; FRAME_HEADER_LEN]> {
    if kind != KIND_CONTROL && kind != KIND_DATA {
        return None;
    }
    if payload_len > MAX_FRAME_PAYLOAD_LEN {
        return None;
    }
    match kind {
        KIND_CONTROL => {
            if stream_id != 0 || payload_len > MAX_CONTROL_JSON_LEN {
                return None;
            }
        }
        KIND_DATA => {
            if stream_id == 0 {
                return None;
            }
        }
        _ => return None,
    }
    let len = u32::try_from(payload_len).ok()?;
    let mut out = [0u8; FRAME_HEADER_LEN];
    out[0] = FRAME_VERSION;
    out[1] = kind;
    out[2..4].copy_from_slice(&0u16.to_be_bytes());
    out[4..8].copy_from_slice(&stream_id.to_be_bytes());
    out[8..12].copy_from_slice(&len.to_be_bytes());
    Some(out)
}

/// Decode and validate a 12-byte header slice.
pub fn decode_header(bytes: &[u8]) -> Result<FrameHeader, FrameError> {
    if bytes.len() < FRAME_HEADER_LEN {
        return Err(FrameError::TruncatedHeader);
    }
    if bytes[0] != FRAME_VERSION {
        return Err(FrameError::Version);
    }
    let kind = bytes[1];
    if kind != KIND_CONTROL && kind != KIND_DATA {
        return Err(FrameError::Kind);
    }
    if u16::from_be_bytes([bytes[2], bytes[3]]) != 0 {
        return Err(FrameError::Flags);
    }
    let stream_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let payload_len = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if payload_len > MAX_FRAME_PAYLOAD_LEN {
        return Err(FrameError::Oversize);
    }
    match kind {
        KIND_CONTROL => {
            if stream_id != 0 {
                return Err(FrameError::ControlStreamId);
            }
            if payload_len > MAX_CONTROL_JSON_LEN {
                return Err(FrameError::ControlOversize);
            }
        }
        KIND_DATA => {
            if stream_id == 0 {
                return Err(FrameError::DataStreamId);
            }
        }
        _ => return Err(FrameError::Kind),
    }
    Ok(FrameHeader {
        kind,
        stream_id,
        payload_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_control_header_round_trips() {
        let h = encode_header(KIND_CONTROL, 0, 12).expect("valid control");
        assert_eq!(&h[0..2], &[1, 1]);
        assert_eq!(&h[2..4], &[0, 0]);
        assert_eq!(&h[4..8], &[0, 0, 0, 0]);
        assert_eq!(&h[8..12], &[0, 0, 0, 12]);
        assert_eq!(
            decode_header(&h).expect("decode"),
            FrameHeader {
                kind: KIND_CONTROL,
                stream_id: 0,
                payload_len: 12
            }
        );
    }

    #[test]
    fn golden_data_header_round_trips() {
        let h = encode_header(KIND_DATA, 7, 65536).expect("max data");
        assert_eq!(
            decode_header(&h).expect("decode"),
            FrameHeader {
                kind: KIND_DATA,
                stream_id: 7,
                payload_len: 65536
            }
        );
    }

    #[test]
    fn max_plus_one_rejected() {
        assert!(encode_header(KIND_DATA, 1, MAX_FRAME_PAYLOAD_LEN + 1).is_none());
        assert!(encode_header(KIND_CONTROL, 0, MAX_CONTROL_JSON_LEN + 1).is_none());
        let mut h = encode_header(KIND_DATA, 1, 10).expect("valid");
        h[8..12].copy_from_slice(&(MAX_FRAME_PAYLOAD_LEN as u32 + 1).to_be_bytes());
        assert_eq!(decode_header(&h), Err(FrameError::Oversize));
    }

    #[test]
    fn directional_violations_rejected() {
        assert!(encode_header(KIND_CONTROL, 1, 10).is_none());
        assert!(encode_header(KIND_DATA, 0, 10).is_none());
        let mut h = encode_header(KIND_CONTROL, 0, 10).expect("valid");
        h[4..8].copy_from_slice(&3u32.to_be_bytes());
        assert_eq!(decode_header(&h), Err(FrameError::ControlStreamId));
        let mut h = encode_header(KIND_DATA, 5, 10).expect("valid");
        h[4..8].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(decode_header(&h), Err(FrameError::DataStreamId));
    }

    #[test]
    fn malformed_headers_rejected() {
        assert_eq!(decode_header(&[0u8; 11]), Err(FrameError::TruncatedHeader));
        let mut h = encode_header(KIND_DATA, 1, 1).expect("valid");
        h[0] = 2;
        assert_eq!(decode_header(&h), Err(FrameError::Version));
        h = encode_header(KIND_DATA, 1, 1).expect("valid");
        h[1] = 9;
        assert_eq!(decode_header(&h), Err(FrameError::Kind));
        h = encode_header(KIND_DATA, 1, 1).expect("valid");
        h[2] = 1;
        assert_eq!(decode_header(&h), Err(FrameError::Flags));
    }
}
