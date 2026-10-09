//! Trusted managed-app launch-context parser.
//!
//! Upstream's production catalog injects the launch identity as two reserved
//! application arguments:
//!
//! ```text
//! --i2pr-app-id=<bounded AppId>
//! --i2pr-app-instance=<canonical-u128-decimal>
//! ```
//!
//! Each must appear exactly once. Malformed, duplicate or missing values fail
//! before any handshake byte is written. The two reserved arguments are removed
//! from the remaining application argument view. No environment variable or
//! executable name is used as identity proof.

use thiserror::Error;

/// Maximum accepted `AppId` length in bytes.
pub const MAX_APP_ID_LEN: usize = 128;
/// Maximum accepted decimal length for the canonical `u128` instance id.
/// `u128::MAX` is 39 decimal digits; anything longer cannot be in range.
pub const MAX_INSTANCE_DECIMAL_LEN: usize = 39;

/// Trusted launch identity derived from the two reserved arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedLaunchIdentity {
    /// Bounded application id exactly as launched.
    pub app_id: String,
    /// Canonical nonzero `u128` instance id.
    pub instance_id: u128,
    /// Decimal rendering exactly as launched (no leading zeros by construction).
    pub instance_decimal: String,
}

/// Launch parse failures. Variants carry only static reasons and bounded
/// counts, never the supplied argument bytes.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum LaunchError {
    /// A required reserved argument is absent.
    #[error("missing required launch argument")]
    Missing,
    /// A reserved argument appears more than once.
    #[error("duplicate launch argument")]
    Duplicate,
    /// A reserved argument value violates its grammar.
    #[error("malformed launch argument")]
    Malformed,
}

/// Validate an `AppId` value.
///
/// Grammar frozen for M012: length `1..=128`, ASCII alphanumeric plus
/// `.` `-` `_` `/`, must start with ASCII alphanumeric, must not contain
/// `..` or `//`, must not contain whitespace. This is deliberately stricter
/// than any upstream catalog: an id that passes here is safe to echo into a
/// `hello` control payload without further escaping concerns.
pub fn validate_app_id(value: &str) -> Result<(), LaunchError> {
    if value.is_empty() || value.len() > MAX_APP_ID_LEN {
        return Err(LaunchError::Malformed);
    }
    let bytes = value.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() {
        return Err(LaunchError::Malformed);
    }
    for b in bytes {
        let ok = b.is_ascii_alphanumeric() || *b == b'.' || *b == b'-' || *b == b'_' || *b == b'/';
        if !ok {
            return Err(LaunchError::Malformed);
        }
    }
    if value.contains("..") || value.contains("//") {
        return Err(LaunchError::Malformed);
    }
    Ok(())
}

/// Validate a canonical nonzero `u128` decimal string.
///
/// Rejects sign, whitespace, leading zeros, exponent/fraction syntax and any
/// value above `u128::MAX`. The string `"0"` is rejected: instance ids are
/// nonzero. The single digit `"0"` would otherwise be ambiguous with an
/// absent identity.
pub fn validate_instance_decimal(value: &str) -> Result<u128, LaunchError> {
    if value.is_empty() || value.len() > MAX_INSTANCE_DECIMAL_LEN {
        return Err(LaunchError::Malformed);
    }
    let bytes = value.as_bytes();
    for b in bytes {
        if !b.is_ascii_digit() {
            return Err(LaunchError::Malformed);
        }
    }
    if value == "0" {
        return Err(LaunchError::Malformed);
    }
    if value.len() > 1 && value.starts_with('0') {
        return Err(LaunchError::Malformed);
    }
    value
        .parse::<u128>()
        .map_err(|_| LaunchError::Malformed)
        .and_then(|v| {
            if v == 0 {
                Err(LaunchError::Malformed)
            } else {
                Ok(v)
            }
        })
}

/// Parse the trusted launch identity from a process argument slice.
///
/// `args` is the full argument vector including the executable name at
/// position 0 when present. The two reserved `--i2pr-app-id=` and
/// `--i2pr-app-instance=` arguments are consumed exactly once each; every
/// other argument is returned in order as the remaining application view.
pub fn parse_launch_args(
    args: &[String],
) -> Result<(ManagedLaunchIdentity, Vec<String>), LaunchError> {
    const ID_PREFIX: &str = "--i2pr-app-id=";
    const INSTANCE_PREFIX: &str = "--i2pr-app-instance=";
    let mut app_id: Option<String> = None;
    let mut instance: Option<(u128, String)> = None;
    let mut rest = Vec::new();
    for arg in args {
        if let Some(value) = arg.strip_prefix(ID_PREFIX) {
            if app_id.is_some() {
                return Err(LaunchError::Duplicate);
            }
            validate_app_id(value)?;
            app_id = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix(INSTANCE_PREFIX) {
            if instance.is_some() {
                return Err(LaunchError::Duplicate);
            }
            let parsed = validate_instance_decimal(value)?;
            instance = Some((parsed, value.to_string()));
        } else {
            rest.push(arg.clone());
        }
    }
    match (app_id, instance) {
        (Some(app_id), Some((instance_id, instance_decimal))) => Ok((
            ManagedLaunchIdentity {
                app_id,
                instance_id,
                instance_decimal,
            },
            rest,
        )),
        _ => Err(LaunchError::Missing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_identity_parses_and_strips_reserved() {
        let (id, rest) = parse_launch_args(&args(&[
            "i2pr-mail",
            "--i2pr-app-id=mail/postman.client",
            "--i2pr-app-instance=42",
            "--mailbox=default",
        ]))
        .expect("valid launch");
        assert_eq!(id.app_id, "mail/postman.client");
        assert_eq!(id.instance_id, 42);
        assert_eq!(id.instance_decimal, "42");
        assert_eq!(rest, args(&["i2pr-mail", "--mailbox=default"]));
    }

    #[test]
    fn missing_identity_fails() {
        assert_eq!(
            parse_launch_args(&args(&["i2pr-mail", "--i2pr-app-id=a"])),
            Err(LaunchError::Missing)
        );
        assert_eq!(
            parse_launch_args(&args(&["i2pr-mail", "--i2pr-app-instance=7"])),
            Err(LaunchError::Missing)
        );
        assert_eq!(
            parse_launch_args(&args(&["i2pr-mail"])),
            Err(LaunchError::Missing)
        );
    }

    #[test]
    fn duplicate_identity_fails() {
        assert_eq!(
            parse_launch_args(&args(&[
                "x",
                "--i2pr-app-id=a",
                "--i2pr-app-id=b",
                "--i2pr-app-instance=1"
            ])),
            Err(LaunchError::Duplicate)
        );
        assert_eq!(
            parse_launch_args(&args(&[
                "x",
                "--i2pr-app-id=a",
                "--i2pr-app-instance=1",
                "--i2pr-app-instance=2"
            ])),
            Err(LaunchError::Duplicate)
        );
    }

    #[test]
    fn malformed_app_ids_fail() {
        for bad in [
            "",
            " leading",
            "-leading-dash",
            ".leading-dot",
            "has space",
            "semi;colon",
            "quote\"x",
            "back\\slash",
            "plus+x",
            "a..b",
            "a//b",
            "trailing\n",
            "tab\there",
        ] {
            assert!(validate_app_id(bad).is_err(), "must reject {bad:?}");
        }
        assert!(validate_app_id(&"a".repeat(MAX_APP_ID_LEN + 1)).is_err());
        assert!(validate_app_id(&"a".repeat(MAX_APP_ID_LEN)).is_ok());
    }

    #[test]
    fn malformed_instance_decimals_fail() {
        for bad in [
            "",
            "0",
            "00",
            "01",
            " 42",
            "42 ",
            "+42",
            "-42",
            "4.2",
            "4e2",
            "0x2a",
            "٤٢",
            "4294967296x",
            "340282366920938463463374607431768211456",
        ] {
            assert!(
                validate_instance_decimal(bad).is_err(),
                "must reject {bad:?}"
            );
        }
        assert_eq!(validate_instance_decimal("1"), Ok(1));
        assert_eq!(
            validate_instance_decimal("340282366920938463463374607431768211455"),
            Ok(u128::MAX)
        );
    }

    #[test]
    fn error_display_carries_no_argument_bytes() {
        // Sentinels must never appear in error rendering.
        let sentinel = "SECRET-CREDENTIAL-PAYLOAD";
        let err = validate_app_id(&format!("bad id {sentinel}")).unwrap_err();
        assert!(!format!("{err}").contains(sentinel));
        let err = parse_launch_args(&args(&["x"])).unwrap_err();
        assert!(!format!("{err}").contains(sentinel));
    }
}
