//! Shared validation for machine names (schema, prompt and source names).

/// Whether `name` is 1–64 bytes of `[a-z0-9.-]`, starting with a letter and not
/// ending with `.` or `-`.
pub(crate) fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() <= 64
        && first.is_ascii_lowercase()
        && !matches!(last, b'.' | b'-')
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
}
