//! Application-role handshake codec.
//!
//! Frozen bytes: magic `I2PA`, major `1`, minor `0`, role Application (`1`),
//! one zero reserved byte — 8 bytes total. Any deviation (wrong magic,
//! version, role, or nonzero reserved byte) is a fatal contract error and the
//! channel must not proceed to `hello`.

use thiserror::Error;

/// Handshake magic `I2PA`.
pub const HANDSHAKE_MAGIC: [u8; 4] = *b"I2PA";
/// Frozen major version.
pub const HANDSHAKE_MAJOR: u8 = 1;
/// Frozen minor version.
pub const HANDSHAKE_MINOR: u8 = 0;
/// Application role value.
pub const ROLE_APPLICATION: u8 = 1;
/// Total handshake length in bytes.
pub const HANDSHAKE_LEN: usize = 8;

/// Handshake failures carry only static reasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum HandshakeError {
    /// Input is not exactly 8 bytes.
    #[error("handshake length mismatch")]
    Length,
    /// Magic bytes differ.
    #[error("handshake magic mismatch")]
    Magic,
    /// Major/minor version differs.
    #[error("handshake version mismatch")]
    Version,
    /// Role is not Application.
    #[error("handshake role mismatch")]
    Role,
    /// Reserved byte is nonzero.
    #[error("handshake reserved byte nonzero")]
    Reserved,
}

/// Encode the exact 8 application handshake bytes.
pub fn encode_handshake() -> [u8; HANDSHAKE_LEN] {
    [
        HANDSHAKE_MAGIC[0],
        HANDSHAKE_MAGIC[1],
        HANDSHAKE_MAGIC[2],
        HANDSHAKE_MAGIC[3],
        HANDSHAKE_MAJOR,
        HANDSHAKE_MINOR,
        ROLE_APPLICATION,
        0,
    ]
}

/// Decode and strictly validate exactly 8 handshake bytes.
pub fn decode_handshake(bytes: &[u8]) -> Result<(), HandshakeError> {
    if bytes.len() != HANDSHAKE_LEN {
        return Err(HandshakeError::Length);
    }
    if bytes[0..4] != HANDSHAKE_MAGIC {
        return Err(HandshakeError::Magic);
    }
    if bytes[4] != HANDSHAKE_MAJOR || bytes[5] != HANDSHAKE_MINOR {
        return Err(HandshakeError::Version);
    }
    if bytes[6] != ROLE_APPLICATION {
        return Err(HandshakeError::Role);
    }
    if bytes[7] != 0 {
        return Err(HandshakeError::Reserved);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_bytes_are_exact() {
        assert_eq!(
            encode_handshake(),
            [0x49, 0x32, 0x50, 0x41, 1, 0, 1, 0],
            "magic I2PA + 1.0 + role Application + zero reserved"
        );
        decode_handshake(&encode_handshake()).expect("golden must decode");
    }

    #[test]
    fn wrong_magic_rejected() {
        for bad in [*b"X2PA", *b"I2PX", *b"i2pa", *b"\0\0\0\0"] {
            let mut h = encode_handshake();
            h[0..4].copy_from_slice(&bad);
            assert_eq!(decode_handshake(&h), Err(HandshakeError::Magic));
        }
    }

    #[test]
    fn wrong_version_rejected() {
        for (major, minor) in [(0, 0), (2, 0), (1, 1), (0, 1), (255, 255)] {
            let mut h = encode_handshake();
            h[4] = major;
            h[5] = minor;
            assert_eq!(decode_handshake(&h), Err(HandshakeError::Version));
        }
    }

    #[test]
    fn wrong_role_rejected() {
        for role in [0, 2, 3, 42, 255] {
            let mut h = encode_handshake();
            h[6] = role;
            assert_eq!(decode_handshake(&h), Err(HandshakeError::Role));
        }
    }

    #[test]
    fn nonzero_reserved_rejected() {
        for r in [1, 0x7f, 0xff] {
            let mut h = encode_handshake();
            h[7] = r;
            assert_eq!(decode_handshake(&h), Err(HandshakeError::Reserved));
        }
    }

    #[test]
    fn length_mismatch_rejected() {
        assert_eq!(decode_handshake(&[]), Err(HandshakeError::Length));
        assert_eq!(decode_handshake(&[0u8; 7]), Err(HandshakeError::Length));
        assert_eq!(decode_handshake(&[0u8; 9]), Err(HandshakeError::Length));
    }
}
