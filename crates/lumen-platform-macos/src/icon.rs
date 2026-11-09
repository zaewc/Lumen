//! An application's icon, drawn small for the Apps page (roadmap 16.4).
//!
//! [`app_icon_png`] asks the system for the icon Finder shows
//! (`-[NSWorkspace iconForFile:]`), draws it into a 64 × 64 pixel bitmap
//! (32 points at 2×) and returns it as PNG. It is display only: it opens no
//! window, changes nothing, and is never sent to AI review. A bundle that is
//! a cloud placeholder is left alone rather than downloaded.

#![allow(unsafe_code)]

use std::os::macos::fs::MetadataExt as _;
use std::path::Path;

use objc2::AnyThread as _;
use objc2::rc::autoreleasepool;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace,
    NSGraphicsContext, NSImageInterpolation, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

use crate::entry::SF_DATALESS;

/// The icon's side in pixels: 32 points at 2×, the size Retina screens show.
pub const ICON_PIXELS: u16 = 64;

/// The largest PNG returned. A 64 × 64 RGBA image is 16 KiB unpacked, so an
/// honest encoding stays well below this; anything larger is dropped.
pub const MAX_ICON_BYTES: usize = 32 * 1024;

/// The icon of the application bundle at `bundle`, 64 × 64 pixels as PNG, or
/// `None` if `bundle` is not a directory, is a cloud placeholder (dataless),
/// or the icon could not be drawn or came out larger than
/// [`MAX_ICON_BYTES`].
#[must_use]
pub fn app_icon_png(bundle: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::symlink_metadata(bundle).ok()?;
    if !meta.is_dir() || meta.st_flags() & SF_DATALESS != 0 {
        return None;
    }
    let path = NSString::from_str(bundle.to_str()?);
    let png = autoreleasepool(|_| draw(&path))?;
    (png.len() <= MAX_ICON_BYTES).then_some(png)
}

/// Draws the icon for `path` into a fresh bitmap and encodes it.
fn draw(path: &NSString) -> Option<Vec<u8>> {
    let pixels = isize::try_from(ICON_PIXELS).ok()?;
    let side = f64::from(ICON_PIXELS / 2);
    let image = NSWorkspace::sharedWorkspace().iconForFile(path);
    // SAFETY: invariant: `planes` is null or points to plane buffers. Why
    // unsafe: the initializer takes a raw pointer. Why it holds: it is null,
    // so the bitmap allocates and owns its own buffer.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            pixels,
            pixels,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }?;
    // 32 points in 64 pixels: drawn at 2×, the size Retina screens show.
    rep.setSize(NSSize::new(side, side));
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    // The current context is per thread; it is put back however drawing goes.
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    context.setImageInterpolation(NSImageInterpolation::High);
    image.drawInRect_fromRect_operation_fraction(
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side)),
        NSRect::ZERO,
        NSCompositingOperation::SourceOver,
        1.0,
    );
    context.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();
    // SAFETY: invariant: the dictionary maps property keys to objects. Why
    // unsafe: the binding cannot check the dictionary's generic types. Why it
    // holds: it is empty.
    let data = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(data.to_vec())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

    /// The PNG's width and height, from its IHDR chunk.
    fn size(png: &[u8]) -> (u32, u32) {
        let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
        (word(16), word(20))
    }

    #[test]
    fn a_folder_gets_a_small_png() {
        // A plain folder has the generic folder icon; no real app is needed.
        let dir = tempfile::tempdir().unwrap();
        let png = app_icon_png(dir.path()).unwrap();
        assert!(png.starts_with(PNG_SIGNATURE));
        assert_eq!(size(&png), (u32::from(ICON_PIXELS), u32::from(ICON_PIXELS)));
        assert!(png.len() <= MAX_ICON_BYTES);
    }

    #[test]
    fn a_file_or_a_missing_path_gets_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(app_icon_png(&file), None);
        assert_eq!(app_icon_png(&dir.path().join("Gone.app")), None);
    }
}
