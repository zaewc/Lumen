//! Escaping of untrusted text for display.
//!
//! Shared by [`RawPath`](crate::RawPath) and [`UntrustedText`](crate::UntrustedText):
//! everything attacker-controlled is rendered through the same rules, so a file
//! name and an application display name cannot smuggle different tricks past the
//! UI or into Jev prompts.

/// Opening delimiter of display escapes. Chosen because it is rare in real file
/// names and unrelated to path separators on every platform. A literal occurrence
/// is itself escaped, so every escape in a display string is unambiguous.
pub(crate) const ESC_OPEN: char = '⟦';
pub(crate) const ESC_CLOSE: char = '⟧';

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

pub(crate) fn push_display_char(out: &mut String, c: char) {
    if needs_escape(c) {
        push_escape(out, Escape::CodePoint(u32::from(c)));
    } else {
        out.push(c);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Escape {
    CodePoint(u32),
    Byte(u8),
}

pub(crate) fn push_escape(out: &mut String, escape: Escape) {
    use std::fmt::Write as _;
    let written = match escape {
        Escape::CodePoint(cp) => write!(out, "{ESC_OPEN}U+{cp:04X}{ESC_CLOSE}"),
        Escape::Byte(b) => write!(out, "{ESC_OPEN}0x{b:02X}{ESC_CLOSE}"),
    };
    // Formatting into a String cannot fail.
    debug_assert!(written.is_ok());
}
