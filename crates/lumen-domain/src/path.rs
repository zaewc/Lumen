//! Byte-exact paths and safe display.
//!
//! File names are attacker-controlled text. Lumen therefore:
//!
//! - stores paths exactly as the operating system returned them ([`RawPath`]):
//!   arbitrary bytes on Unix-like systems, UTF-16 code units (possibly with
//!   unpaired surrogates) on Windows, kept losslessly as WTF-8;
//! - never makes decisions from path text (decisions key on
//!   [`FileIdentity`](crate::FileIdentity));
//! - shows paths to people and to Jev only through [`RawPath::display`], which
//!   produces a single line in which control characters, line separators,
//!   bidirectional overrides, invisible characters and undecodable data are
//!   visible as escapes (threat model, TB1).
//!
//! Conversion to `OsString` belongs to the platform adapters, keeping this crate
//! portable.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Opening delimiter of display escapes. Chosen because it is rare in real file
/// names and unrelated to path separators on every platform. A literal occurrence
/// is itself escaped, so every escape in a display string is unambiguous.
const ESC_OPEN: char = '⟦';
const ESC_CLOSE: char = '⟧';

/// Which platform's path model a [`RawPath`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathFlavor {
    /// Arbitrary bytes, `/`-separated (macOS, Linux, Android, iOS).
    Unix,
    /// UTF-16 code units, `\`-separated, stored as WTF-8.
    Windows,
}

/// Error returned when raw path data is invalid for its flavor.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PathError {
    /// Windows path bytes are not well-formed WTF-8.
    #[error("Windows path data is not well-formed WTF-8 at byte {index}")]
    InvalidWtf8 {
        /// Byte offset of the first invalid sequence.
        index: usize,
    },
    /// The serialized hex payload is malformed.
    #[error("path hex payload is malformed")]
    InvalidHex,
    /// Path data contains a NUL, which no supported platform allows in paths.
    #[error("path contains a NUL at byte {index}")]
    ContainsNul {
        /// Byte offset of the NUL.
        index: usize,
    },
}

/// A path exactly as the operating system reported it.
///
/// Equality is byte equality, which is deliberately **not** "same file": two
/// different byte strings can name the same file (case folding, Unicode
/// normalisation, hard links), and the same bytes can name different files over
/// time. Use [`FileIdentity`](crate::FileIdentity) to decide sameness.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawPath {
    flavor: PathFlavor,
    bytes: Vec<u8>,
}

impl RawPath {
    /// Wraps Unix path bytes.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::ContainsNul`] if the bytes contain a NUL.
    pub fn from_unix_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, PathError> {
        let bytes = bytes.into();
        reject_nul(&bytes)?;
        Ok(Self {
            flavor: PathFlavor::Unix,
            bytes,
        })
    }

    /// Wraps Windows path UTF-16 code units (unpaired surrogates allowed).
    ///
    /// # Errors
    ///
    /// Returns [`PathError::ContainsNul`] if the units contain a NUL.
    pub fn from_windows_wide(units: &[u16]) -> Result<Self, PathError> {
        if let Some(index) = units.iter().position(|&u| u == 0) {
            return Err(PathError::ContainsNul { index });
        }
        Ok(Self {
            flavor: PathFlavor::Windows,
            bytes: wtf8_encode(units),
        })
    }

    /// Reconstructs a path from its flavor and stored bytes (e.g. from the
    /// database), validating the bytes for the flavor.
    ///
    /// # Errors
    ///
    /// Returns a [`PathError`] if the bytes contain a NUL or, for Windows paths,
    /// are not well-formed WTF-8.
    pub fn from_stored(flavor: PathFlavor, bytes: Vec<u8>) -> Result<Self, PathError> {
        reject_nul(&bytes)?;
        if flavor == PathFlavor::Windows {
            wtf8_decode(&bytes)?;
        }
        Ok(Self { flavor, bytes })
    }

    /// The path model.
    pub const fn flavor(&self) -> PathFlavor {
        self.flavor
    }

    /// Stored bytes: raw bytes for Unix, WTF-8 for Windows.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The original UTF-16 code units of a Windows path; `None` for Unix paths.
    pub fn to_windows_wide(&self) -> Option<Vec<u16>> {
        match self.flavor {
            PathFlavor::Unix => None,
            // Validated on construction, so decoding cannot fail.
            PathFlavor::Windows => wtf8_decode(&self.bytes).ok().map(|cps| wtf16_units(&cps)),
        }
    }

    /// A single-line, human- and model-safe rendering of the path.
    ///
    /// Printable text is kept as is. Control characters (including newlines and
    /// tabs), Unicode line separators, bidirectional formatting characters,
    /// default-ignorable (invisible) characters and unpaired surrogates appear as
    /// `⟦U+XXXX⟧`; bytes that are not valid UTF-8 appear as `⟦0xXX⟧`; a literal
    /// `⟦` appears as `⟦U+27E6⟧`. Distinct paths of the same flavor always
    /// render differently.
    pub fn display(&self) -> String {
        let mut out = String::with_capacity(self.bytes.len());
        match self.flavor {
            PathFlavor::Unix => {
                for chunk in self.bytes.utf8_chunks() {
                    chunk
                        .valid()
                        .chars()
                        .for_each(|c| push_display_char(&mut out, c));
                    for byte in chunk.invalid() {
                        push_escape(&mut out, Escape::Byte(*byte));
                    }
                }
            }
            PathFlavor::Windows => {
                for cp in wtf8_decode(&self.bytes).unwrap_or_default() {
                    match char::from_u32(cp) {
                        Some(c) => push_display_char(&mut out, c),
                        None => push_escape(&mut out, Escape::CodePoint(cp)),
                    }
                }
            }
        }
        out
    }
}

impl fmt::Debug for RawPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawPath({:?}, {:?})", self.flavor, self.display())
    }
}

fn reject_nul(bytes: &[u8]) -> Result<(), PathError> {
    match bytes.iter().position(|&b| b == 0) {
        Some(index) => Err(PathError::ContainsNul { index }),
        None => Ok(()),
    }
}

/// Whether a character must be escaped in display output.
pub(crate) fn needs_escape(c: char) -> bool {
    c == ESC_OPEN
        || c.is_control()
        || matches!(c, '\u{2028}' | '\u{2029}')
        || is_default_ignorable(c)
}

/// Unicode `Default_Ignorable_Code_Point` (DerivedCoreProperties): characters
/// that render invisibly. Includes all bidirectional formatting characters
/// (U+061C, U+200E–U+200F, U+202A–U+202E, U+2066–U+2069).
fn is_default_ignorable(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

fn push_display_char(out: &mut String, c: char) {
    if needs_escape(c) {
        push_escape(out, Escape::CodePoint(u32::from(c)));
    } else {
        out.push(c);
    }
}

#[derive(Clone, Copy)]
enum Escape {
    CodePoint(u32),
    Byte(u8),
}

fn push_escape(out: &mut String, escape: Escape) {
    use std::fmt::Write as _;
    let written = match escape {
        Escape::CodePoint(cp) => write!(out, "{ESC_OPEN}U+{cp:04X}{ESC_CLOSE}"),
        Escape::Byte(b) => write!(out, "{ESC_OPEN}0x{b:02X}{ESC_CLOSE}"),
    };
    // Formatting into a String cannot fail.
    debug_assert!(written.is_ok());
}

/// Encodes UTF-16 code units as WTF-8: paired surrogates become one 4-byte
/// sequence; unpaired surrogates use the 3-byte generalized UTF-8 form.
fn wtf8_encode(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(units.len());
    for unit in char::decode_utf16(units.iter().copied()) {
        let cp = match unit {
            Ok(c) => u32::from(c),
            Err(e) => u32::from(e.unpaired_surrogate()),
        };
        push_generalized_utf8(&mut out, cp);
    }
    out
}

#[allow(clippy::cast_possible_truncation)] // Each value is masked to 8 bits first.
fn push_generalized_utf8(out: &mut Vec<u8>, cp: u32) {
    match cp {
        0..=0x7F => out.push(cp as u8),
        0x80..=0x7FF => out.extend([0xC0 | (cp >> 6) as u8, 0x80 | (cp & 0x3F) as u8]),
        0x800..=0xFFFF => out.extend([
            0xE0 | (cp >> 12) as u8,
            0x80 | ((cp >> 6) & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
        ]),
        _ => out.extend([
            0xF0 | (cp >> 18) as u8,
            0x80 | ((cp >> 12) & 0x3F) as u8,
            0x80 | ((cp >> 6) & 0x3F) as u8,
            0x80 | (cp & 0x3F) as u8,
        ]),
    }
}

/// Strictly decodes WTF-8 into code points (surrogates included). Rejects
/// overlong forms, out-of-range values, and a lead surrogate immediately followed
/// by a trail surrogate (which WTF-8 requires to be encoded as one supplementary
/// code point), so every code point sequence has exactly one encoding.
fn wtf8_decode(bytes: &[u8]) -> Result<Vec<u32>, PathError> {
    let mut cps = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let err = PathError::InvalidWtf8 { index: i };
        let b0 = bytes[i];
        let (len, min, init) = match b0 {
            0x00..=0x7F => (1, 0, u32::from(b0)),
            0xC2..=0xDF => (2, 0x80, u32::from(b0 & 0x1F)),
            0xE0..=0xEF => (3, 0x800, u32::from(b0 & 0x0F)),
            0xF0..=0xF4 => (4, 0x1_0000, u32::from(b0 & 0x07)),
            _ => return Err(err),
        };
        let tail = bytes.get(i + 1..i + len).ok_or(err.clone())?;
        let mut cp = init;
        for &b in tail {
            if b & 0xC0 != 0x80 {
                return Err(err);
            }
            cp = (cp << 6) | u32::from(b & 0x3F);
        }
        if cp < min || cp > 0x10_FFFF {
            return Err(err);
        }
        let is_trail = (0xDC00..=0xDFFF).contains(&cp);
        if is_trail
            && cps
                .last()
                .is_some_and(|prev| (0xD800..=0xDBFF).contains(prev))
        {
            return Err(err);
        }
        cps.push(cp);
        i += len;
    }
    Ok(cps)
}

#[allow(clippy::cast_possible_truncation)] // Values are within u16 range by construction.
fn wtf16_units(cps: &[u32]) -> Vec<u16> {
    let mut units = Vec::with_capacity(cps.len());
    for &cp in cps {
        if cp >= 0x1_0000 {
            let v = cp - 0x1_0000;
            units.push(0xD800 | (v >> 10) as u16);
            units.push(0xDC00 | (v & 0x3FF) as u16);
        } else {
            units.push(cp as u16);
        }
    }
    units
}

/// Serialized form: flavor plus lowercase hex of the stored bytes. Hex keeps the
/// exact bytes (quarantine manifests must restore the original name) without a
/// dependency; the display form is for people and is not serialized here.
#[derive(Serialize, Deserialize)]
struct Wire {
    flavor: PathFlavor,
    hex: String,
}

impl Serialize for RawPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut hex = String::with_capacity(self.bytes.len() * 2);
        for &b in &self.bytes {
            hex.push(char::from(DIGITS[usize::from(b >> 4)]));
            hex.push(char::from(DIGITS[usize::from(b & 0x0F)]));
        }
        Wire {
            flavor: self.flavor,
            hex,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RawPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = Wire::deserialize(deserializer)?;
        let bytes = decode_hex(&wire.hex).map_err(serde::de::Error::custom)?;
        Self::from_stored(wire.flavor, bytes).map_err(serde::de::Error::custom)
    }
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, PathError> {
    if !hex.len().is_multiple_of(2) {
        return Err(PathError::InvalidHex);
    }
    let nibble = |b: u8| match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(PathError::InvalidHex),
    };
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[hi, lo]| Ok((nibble(hi)? << 4) | nibble(lo)?))
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn unix(bytes: &[u8]) -> Result<RawPath, PathError> {
        RawPath::from_unix_bytes(bytes.to_vec())
    }

    #[test]
    fn display_keeps_ordinary_text() -> Result<(), PathError> {
        assert_eq!(
            unix(b"/Users/ana/Library/Caches/com.example.App")?.display(),
            "/Users/ana/Library/Caches/com.example.App"
        );
        assert_eq!(
            unix("/tmp/사진 2026 (1).jpg".as_bytes())?.display(),
            "/tmp/사진 2026 (1).jpg"
        );
        Ok(())
    }

    #[test]
    fn display_escapes_deceptive_characters() -> Result<(), PathError> {
        // Right-to-left override: "invoice\u{202E}fdp.exe" would render as "invoiceexe.pdf".
        assert_eq!(
            unix("/tmp/invoice\u{202E}fdp.exe".as_bytes())?.display(),
            "/tmp/invoice⟦U+202E⟧fdp.exe"
        );
        assert_eq!(
            unix(b"/tmp/a\nb\tc\x1b[31m")?.display(),
            "/tmp/a⟦U+000A⟧b⟦U+0009⟧c⟦U+001B⟧[31m"
        );
        assert_eq!(
            unix("/x/zero\u{200B}width\u{FEFF}".as_bytes())?.display(),
            "/x/zero⟦U+200B⟧width⟦U+FEFF⟧"
        );
        assert_eq!(
            unix("/x/line\u{2028}sep\u{0085}".as_bytes())?.display(),
            "/x/line⟦U+2028⟧sep⟦U+0085⟧"
        );
        assert_eq!(
            unix("/x/tag\u{E0041}".as_bytes())?.display(),
            "/x/tag⟦U+E0041⟧"
        );
        Ok(())
    }

    #[test]
    fn display_escapes_invalid_utf8_and_the_escape_bracket() -> Result<(), PathError> {
        assert_eq!(unix(b"/tmp/\xff\xfeok")?.display(), "/tmp/⟦0xFF⟧⟦0xFE⟧ok");
        assert_eq!(
            unix("/tmp/⟦U+202E⟧".as_bytes())?.display(),
            "/tmp/⟦U+27E6⟧U+202E⟧"
        );
        Ok(())
    }

    #[test]
    fn display_is_single_line() -> Result<(), PathError> {
        let p = unix(b"/a\r\nb\x0bc\x0cd\xc2\x85e")?;
        assert!(
            !p.display()
                .contains(['\r', '\n', '\u{0b}', '\u{0c}', '\u{85}'])
        );
        Ok(())
    }

    #[test]
    fn nul_is_rejected() {
        assert_eq!(
            unix(b"/a\0b").err(),
            Some(PathError::ContainsNul { index: 2 })
        );
        assert_eq!(
            RawPath::from_windows_wide(&[0x43, 0, 0x44]).err(),
            Some(PathError::ContainsNul { index: 1 })
        );
    }

    #[test]
    fn windows_paths_round_trip_unpaired_surrogates() -> Result<(), PathError> {
        let units = [
            u16::from(b'C'),
            u16::from(b':'),
            u16::from(b'\\'),
            0xD800,
            u16::from(b'x'),
            0xD83D,
            0xDE00,
        ];
        let p = RawPath::from_windows_wide(&units)?;
        assert_eq!(p.to_windows_wide().as_deref(), Some(&units[..]));
        assert_eq!(p.display(), "C:\\⟦U+D800⟧x😀");
        assert_eq!(unix(b"/a")?.to_windows_wide(), None);
        Ok(())
    }

    #[test]
    fn stored_windows_bytes_must_be_canonical_wtf8() {
        for bad in [
            &b"\xC0\x80"[..],                 // overlong NUL
            &b"\xE0\x80\x80"[..],             // overlong
            &b"\xF4\x90\x80\x80"[..],         // beyond U+10FFFF
            &b"\xED\xA0\x80\xED\xB0\x80"[..], // paired surrogates encoded separately
            &b"\xE2\x82"[..],                 // truncated
            &b"\x80"[..],                     // lone continuation byte
        ] {
            assert!(
                RawPath::from_stored(PathFlavor::Windows, bad.to_vec()).is_err(),
                "{bad:x?}"
            );
        }
        assert!(
            RawPath::from_stored(PathFlavor::Windows, b"\xED\xA0\x80".to_vec()).is_ok(),
            "lone surrogate is valid WTF-8"
        );
    }

    #[test]
    fn serde_preserves_exact_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let p = unix(b"/tmp/\xffname")?;
        let json = serde_json::to_string(&p)?;
        assert_eq!(json, r#"{"flavor":"unix","hex":"2f746d702fff6e616d65"}"#);
        assert_eq!(serde_json::from_str::<RawPath>(&json)?, p);
        for bad in [
            r#"{"flavor":"unix","hex":"2"}"#,
            r#"{"flavor":"unix","hex":"2G"}"#,
            r#"{"flavor":"unix","hex":"2F"}"#,
            r#"{"flavor":"unix","hex":"2f00"}"#,
            r#"{"flavor":"windows","hex":"c080"}"#,
        ] {
            assert!(serde_json::from_str::<RawPath>(bad).is_err(), "{bad}");
        }
        Ok(())
    }

    proptest! {
        #[test]
        fn windows_wide_round_trips(units in prop::collection::vec(1u16.., 0..64)) {
            let p = RawPath::from_windows_wide(&units).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(p.to_windows_wide(), Some(units));
            let again = RawPath::from_stored(PathFlavor::Windows, p.as_bytes().to_vec());
            prop_assert_eq!(again, Ok(p));
        }

        #[test]
        fn serde_round_trips(bytes in prop::collection::vec(1u8.., 0..64)) {
            let p = RawPath::from_unix_bytes(bytes).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let json = serde_json::to_string(&p).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let back: RawPath = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(back, p);
        }

        #[test]
        fn display_never_contains_dangerous_characters(bytes in prop::collection::vec(1u8.., 0..64)) {
            let p = RawPath::from_unix_bytes(bytes).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let shown = p.display();
            // Escapes use only the brackets themselves plus ASCII.
            let unescaped = shown.replace(ESC_CLOSE, "");
            prop_assert!(!unescaped.chars().any(|c| c != ESC_OPEN && needs_escape(c)), "{shown:?}");
        }

        #[test]
        fn display_is_injective(a in prop::collection::vec(1u8.., 0..24), b in prop::collection::vec(1u8.., 0..24)) {
            let pa = RawPath::from_unix_bytes(a.clone()).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let pb = RawPath::from_unix_bytes(b.clone()).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(pa.display() == pb.display(), a == b);
        }
    }
}
