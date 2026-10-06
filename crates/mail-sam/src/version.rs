//! SAM protocol version values, strict parsing, and version negotiation.

use std::fmt;

/// A `major.minor` SAM protocol version.
///
/// Used by `HELLO VERSION MIN=.. MAX=..` and by the `VERSION=` option on a
/// `HELLO REPLY`. Ordering is lexicographic on `(major, minor)` so
/// [`negotiate`] can pick the highest mutually supported version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SamVersion {
    major: u16,
    minor: u16,
}

/// Oldest SAM version this client will accept.
pub const MIN_SUPPORTED: SamVersion = SamVersion::new(3, 1);
/// Newest SAM version this client will accept. The SAM v3.1 line is the
/// ceiling until an upstream router version is verified against this codec.
pub const MAX_SUPPORTED: SamVersion = SamVersion::new(3, 1);

/// Why a version string could not be parsed, or why negotiation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamVersionError {
    /// Input was empty. An empty version is not a wildcard in SAM; it is
    /// malformed, so it is rejected rather than treated as "any version".
    Empty,
    /// Input contained a control byte, a space, or a non-ASCII byte. Versions
    /// travel unquoted on the wire, so surrounding whitespace would be part of
    /// the token and must not be silently trimmed.
    NotAsciiToken,
    /// Input did not contain exactly one `.` separator.
    Malformed,
    /// Input contained an empty component, as in `3.` or `.1`.
    EmptyComponent,
    /// A component contained a non-decimal byte, such as `3.x` or `-1.2`.
    NotDecimal,
    /// A component exceeded `u16`. Rejected instead of wrapping, because a
    /// wrapped major version could silently negotiate as a lower version.
    ComponentOverflow,
    /// `client_min` was greater than `client_max`, which describes no range.
    InvertedRange,
    /// The client's range and [`MIN_SUPPORTED`]..=[`MAX_SUPPORTED`] do not
    /// overlap, or the overlap spans two major versions. Negotiation fails
    /// closed: falling back to a version the router did not offer is worse
    /// than reporting no session.
    NoCommonVersion,
}

impl SamVersion {
    /// Builds a version from its components.
    pub const fn new(major: u16, minor: u16) -> SamVersion {
        SamVersion { major, minor }
    }

    /// Returns the major component.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor component.
    pub const fn minor(self) -> u16 {
        self.minor
    }
}

impl fmt::Display for SamVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Parses a strict `major.minor` version.
///
/// Every deviation is an error rather than a repaired value: an empty input, a
/// surrounding space, a control byte, a non-ASCII byte, a missing or repeated
/// `.`, an empty component, a non-decimal component, and a `u16` overflow.
pub fn parse_version(input: &str) -> Result<SamVersion, SamVersionError> {
    if input.is_empty() {
        return Err(SamVersionError::Empty);
    }
    // Printable non-space ASCII only: the wire form is an unquoted token, so a
    // space here means the caller passed an already-split fragment.
    if !input.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(SamVersionError::NotAsciiToken);
    }
    let Some((major_text, rest)) = input.split_once('.') else {
        return Err(SamVersionError::Malformed);
    };
    if rest.contains('.') {
        return Err(SamVersionError::Malformed);
    }
    if major_text.is_empty() || rest.is_empty() {
        return Err(SamVersionError::EmptyComponent);
    }
    if !major_text.bytes().all(|b| b.is_ascii_digit()) || !rest.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(SamVersionError::NotDecimal);
    }
    let major = major_text
        .parse::<u16>()
        .map_err(|_| SamVersionError::ComponentOverflow)?;
    let minor = rest
        .parse::<u16>()
        .map_err(|_| SamVersionError::ComponentOverflow)?;
    Ok(SamVersion::new(major, minor))
}

/// Selects the highest version both peers support.
///
/// `client_min`/`client_max` describe what this build can speak; the function
/// intersects that with [`MIN_SUPPORTED`]..=[`MAX_SUPPORTED`]. An empty or
/// inverted intersection, or an intersection spanning two major versions, is a
/// [`SamVersionError::NoCommonVersion`] failure.
pub fn negotiate(
    client_min: SamVersion,
    client_max: SamVersion,
) -> Result<SamVersion, SamVersionError> {
    if client_min > client_max {
        return Err(SamVersionError::InvertedRange);
    }
    let low = if client_min < MIN_SUPPORTED {
        MIN_SUPPORTED
    } else {
        client_min
    };
    let high = if client_max > MAX_SUPPORTED {
        MAX_SUPPORTED
    } else {
        client_max
    };
    if low > high || low.major() != high.major() {
        return Err(SamVersionError::NoCommonVersion);
    }
    Ok(high)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_well_formed_versions() {
        assert_eq!(parse_version("3.1"), Ok(SamVersion::new(3, 1)));
        assert_eq!(parse_version("3.0"), Ok(SamVersion::new(3, 0)));
        assert_eq!(parse_version("0.0"), Ok(SamVersion::new(0, 0)));
        assert_eq!(
            parse_version("65535.65535"),
            Ok(SamVersion::new(65535, 65535))
        );
    }

    #[test]
    fn rejects_malformed_versions() {
        assert_eq!(parse_version(""), Err(SamVersionError::Empty));
        assert_eq!(parse_version(" 3.1"), Err(SamVersionError::NotAsciiToken));
        assert_eq!(parse_version("3.1 "), Err(SamVersionError::NotAsciiToken));
        assert_eq!(parse_version("3.\n1"), Err(SamVersionError::NotAsciiToken));
        assert_eq!(
            parse_version("3.\u{0}1"),
            Err(SamVersionError::NotAsciiToken)
        );
        assert_eq!(
            parse_version("3.\u{fffd}"),
            Err(SamVersionError::NotAsciiToken)
        );
        assert_eq!(parse_version("3"), Err(SamVersionError::Malformed));
        assert_eq!(parse_version("3.1.2"), Err(SamVersionError::Malformed));
        assert_eq!(parse_version("3."), Err(SamVersionError::EmptyComponent));
        assert_eq!(parse_version(".1"), Err(SamVersionError::EmptyComponent));
        assert_eq!(parse_version("."), Err(SamVersionError::EmptyComponent));
        assert_eq!(parse_version("a.b"), Err(SamVersionError::NotDecimal));
        assert_eq!(parse_version("3.1a"), Err(SamVersionError::NotDecimal));
        assert_eq!(parse_version("-1.2"), Err(SamVersionError::NotDecimal));
        assert_eq!(parse_version("+1.2"), Err(SamVersionError::NotDecimal));
        assert_eq!(parse_version("3. 1"), Err(SamVersionError::NotAsciiToken));
    }

    #[test]
    fn rejects_component_overflow_instead_of_wrapping() {
        assert_eq!(
            parse_version("65536.1"),
            Err(SamVersionError::ComponentOverflow)
        );
        assert_eq!(
            parse_version("3.65536"),
            Err(SamVersionError::ComponentOverflow)
        );
        assert_eq!(
            parse_version("99999.99999"),
            Err(SamVersionError::ComponentOverflow)
        );
    }

    #[test]
    fn version_renders_and_compares() {
        assert_eq!(SamVersion::new(3, 1).to_string(), "3.1");
        assert_eq!(MIN_SUPPORTED, SamVersion::new(3, 1));
        assert_eq!(MAX_SUPPORTED, SamVersion::new(3, 1));
        assert!(SamVersion::new(3, 0) < SamVersion::new(3, 1));
        assert!(SamVersion::new(3, 1) > SamVersion::new(2, 9));
        assert_eq!(SamVersion::new(4, 0).major(), 4);
        assert_eq!(SamVersion::new(4, 0).minor(), 0);
    }

    #[test]
    fn negotiation_selects_the_highest_common_version() {
        assert_eq!(
            negotiate(SamVersion::new(3, 1), SamVersion::new(3, 1)),
            Ok(SamVersion::new(3, 1))
        );
        // A client that also speaks older versions still lands on our ceiling.
        assert_eq!(
            negotiate(SamVersion::new(3, 0), SamVersion::new(3, 5)),
            Ok(SamVersion::new(3, 1))
        );
        // A client capped below our floor has no overlap.
        assert_eq!(
            negotiate(SamVersion::new(2, 0), SamVersion::new(2, 9)),
            Err(SamVersionError::NoCommonVersion)
        );
    }

    #[test]
    fn negotiation_fails_closed_on_disjoint_ranges() {
        // Entirely newer client: our ceiling is 3.1.
        assert_eq!(
            negotiate(SamVersion::new(4, 0), SamVersion::new(5, 0)),
            Err(SamVersionError::NoCommonVersion)
        );
        // Overlapping only across a major boundary is refused rather than
        // silently pinning to a major the router never offered.
        assert_eq!(
            negotiate(SamVersion::new(3, 9), SamVersion::new(4, 2)),
            Err(SamVersionError::NoCommonVersion)
        );
        // Inverted client range describes nothing.
        assert_eq!(
            negotiate(SamVersion::new(3, 2), SamVersion::new(3, 0)),
            Err(SamVersionError::InvertedRange)
        );
    }

    #[test]
    fn every_no_overlap_case_is_distinguished_from_an_inverted_range() {
        assert_ne!(
            negotiate(SamVersion::new(4, 0), SamVersion::new(5, 0)),
            negotiate(SamVersion::new(3, 2), SamVersion::new(3, 0))
        );
    }
}
