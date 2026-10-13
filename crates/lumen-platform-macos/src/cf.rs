//! CoreFoundation helpers shared by the inventories: parsing property lists with
//! the system's own parser, and type-checked dictionary lookups. The only
//! `unsafe` here is two FFI calls and borrowing a value its dictionary owns.

#![allow(unsafe_code)]

use std::ffi::c_void;

use objc2_core_foundation::{
    CFArray, CFData, CFDictionary, CFPropertyListCreateWithData, CFRetained, CFString, CFType,
    ConcreteType,
};

/// `kCFPropertyListImmutable`.
const IMMUTABLE: usize = 0;

/// Parses a property list (XML or binary) as macOS itself does. `None` if the
/// bytes are not a property list.
pub(crate) fn property_list(bytes: &[u8]) -> Option<CFRetained<CFType>> {
    let data = CFData::from_bytes(bytes);
    // SAFETY: invariant: `data` is a live CFData, and the format and error
    // out-pointers may be null. Why unsafe: FFI call. Why it holds: both are
    // passed as null, so nothing is written back; the result is adopted by the
    // binding as a +1 reference (Create rule).
    unsafe {
        CFPropertyListCreateWithData(
            None,
            Some(&data),
            IMMUTABLE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    }
}

/// The value for `key` in `dict` if it exists and has type `T`.
pub(crate) fn get<'a, T: ConcreteType>(dict: &'a CFDictionary, key: &CFString) -> Option<&'a T> {
    let key: *const CFString = key;
    // SAFETY: invariant: `dict` is a live dictionary and `key` a live CFString.
    // Why unsafe: untyped dictionary access returning a borrowed pointer. Why it
    // holds: both references are valid for the call; keys are compared with
    // CFEqual, which accepts any CF object.
    let value = unsafe { dict.value(key.cast::<c_void>()) }.cast::<CFType>();
    // SAFETY: invariant: a non-null value is a live CF object owned by `dict`.
    // Why unsafe: turning a raw pointer into a reference. Why it holds: the
    // returned lifetime is tied to `dict`, which owns the value, and
    // `downcast_ref` checks the CF type before the value is used as `T`.
    let value = unsafe { value.as_ref() }?;
    value.downcast_ref::<T>()
}

/// The element at `index` in `array` if it exists and has type `T`.
pub(crate) fn element<T: ConcreteType>(array: &CFArray, index: usize) -> Option<&T> {
    let index = isize::try_from(index).ok()?;
    if index >= array.count() {
        return None;
    }
    // SAFETY: invariant: `index` is within `0..count`. Why unsafe: unchecked,
    // untyped array access returning a borrowed pointer. Why it holds: the bound
    // was checked just above on this immutable array.
    let value = unsafe { array.value_at_index(index) }.cast::<CFType>();
    // SAFETY: invariant: a non-null value is a live CF object owned by `array`.
    // Why unsafe: turning a raw pointer into a reference. Why it holds: the
    // lifetime is tied to `array`, and `downcast_ref` checks the type.
    let value = unsafe { value.as_ref() }?;
    value.downcast_ref::<T>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_property_lists_and_checks_value_types() {
        let xml = b"<?xml version=\"1.0\"?><plist version=\"1.0\"><dict>\
            <key>Label</key><string>com.example.agent</string>\
            <key>Args</key><array><string>/bin/ls</string><integer>3</integer></array>\
            </dict></plist>";
        let list = property_list(xml);
        let dict = list
            .as_deref()
            .and_then(|l| l.downcast_ref::<CFDictionary>());
        let key = |k: &str| CFString::from_str(k);
        let label = dict.and_then(|d| get::<CFString>(d, &key("Label")));
        assert_eq!(
            label.map(ToString::to_string).as_deref(),
            Some("com.example.agent")
        );
        assert!(
            dict.and_then(|d| get::<CFString>(d, &key("Args")))
                .is_none(),
            "an array is not a string"
        );
        assert!(
            dict.and_then(|d| get::<CFString>(d, &key("Missing")))
                .is_none()
        );
        let args = dict.and_then(|d| get::<CFArray>(d, &key("Args")));
        let first = args.and_then(|a| element::<CFString>(a, 0));
        assert_eq!(first.map(ToString::to_string).as_deref(), Some("/bin/ls"));
        assert!(
            args.and_then(|a| element::<CFString>(a, 1)).is_none(),
            "a number"
        );
        assert!(
            args.and_then(|a| element::<CFString>(a, 2)).is_none(),
            "out of range"
        );
    }

    #[test]
    fn rejects_bytes_that_are_not_property_lists() {
        for bad in [
            &b""[..],
            b"not a plist",
            b"<plist><dict><key>x</key></plist>",
            &[0xff; 64],
        ] {
            assert!(property_list(bad).is_none(), "{bad:?}");
        }
    }
}
