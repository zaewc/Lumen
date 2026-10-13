//! Windows path helpers that need no system calls.

const BACKSLASH: u16 = b'\\' as u16;

/// Prefixes an absolute path with `\\?\` (or `\\?\UNC\` for shares) so the Win32
/// layer skips `MAX_PATH` limits and name normalisation; Lumen's paths are
/// already absolute and normalised. Other paths are returned unchanged.
pub fn verbatim(path: &[u16]) -> Vec<u16> {
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().collect() };
    let is_drive = path.len() >= 3
        && matches!(path[0], 0x41..=0x5A | 0x61..=0x7A)
        && path[1] == u16::from(b':')
        && path[2] == BACKSLASH;
    let is_unc = path.starts_with(&[BACKSLASH, BACKSLASH])
        && !path.starts_with(&wide(r"\\?\"))
        && !path.starts_with(&wide(r"\\.\"));
    let mut out = if is_drive {
        wide(r"\\?\")
    } else if is_unc {
        let mut prefix = wide(r"\\?\UNC");
        prefix.extend_from_slice(&path[1..]);
        return prefix;
    } else {
        Vec::new()
    };
    out.extend_from_slice(path);
    out
}

/// Appends the NUL terminator Win32 path parameters need.
pub fn nul_terminated(path: &[u16]) -> Option<Vec<u16>> {
    if path.contains(&0) {
        return None;
    }
    let mut out = path.to_vec();
    out.push(0);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn prefixes_drive_and_share_paths_once() {
        assert_eq!(verbatim(&wide(r"C:\Users\a")), wide(r"\\?\C:\Users\a"));
        assert_eq!(
            verbatim(&wide(r"\\srv\share\x")),
            wide(r"\\?\UNC\srv\share\x")
        );
        assert_eq!(verbatim(&wide(r"\\?\C:\x")), wide(r"\\?\C:\x"));
        assert_eq!(verbatim(&wide(r"\\.\pipe\x")), wide(r"\\.\pipe\x"));
        assert_eq!(verbatim(&wide(r"relative\x")), wide(r"relative\x"));
        assert_eq!(verbatim(&wide("C:")), wide("C:"));
    }

    #[test]
    fn nul_termination_rejects_interior_nul() {
        assert_eq!(nul_terminated(&wide("ab")), Some(vec![97, 98, 0]));
        assert_eq!(nul_terminated(&[97, 0, 98]), None);
    }
}
