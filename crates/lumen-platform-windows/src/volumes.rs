//! Which volumes may hold a quarantine store, and whether this PC is
//! single-user (ADR-0046).
//!
//! A volume gets a store only if it is a local fixed drive that is not
//! removable and not on a USB, IEEE 1394, SD or MMC bus (USB disks also report
//! `DRIVE_FIXED`), formatted NTFS or `ReFS` with persistent ACLs, writable, and
//! with a volume serial number no other mounted volume shares. The decisions
//! are plain functions over the facts, tested on every platform; on Windows,
//! [`mounted`] gathers the facts and [`single_user_pc`] reads the profile list.

/// What `GetDriveTypeW` reports for a root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveType {
    /// `DRIVE_FIXED`: a fixed disk (internal, or a USB disk).
    Fixed,
    /// `DRIVE_REMOVABLE`: removable media (card readers, USB sticks).
    Removable,
    /// `DRIVE_REMOTE`: a network share.
    Remote,
    /// `DRIVE_CDROM`.
    Optical,
    /// `DRIVE_RAMDISK`.
    RamDisk,
    /// `DRIVE_UNKNOWN`, `DRIVE_NO_ROOT_DIR` or anything else.
    Other,
}

impl DriveType {
    /// The type for a `GetDriveTypeW` result.
    pub const fn from_win32(value: u32) -> Self {
        match value {
            2 => Self::Removable,
            3 => Self::Fixed,
            4 => Self::Remote,
            5 => Self::Optical,
            6 => Self::RamDisk,
            _ => Self::Other,
        }
    }
}

/// Buses whose disks can be unplugged and carried away
/// (`STORAGE_BUS_TYPE`: `BusType1394`, `BusTypeUsb`, `BusTypeSd`,
/// `BusTypeMmc`).
const DETACHABLE_BUSES: [i32; 4] = [4, 7, 12, 13];

/// What the storage device under a volume says about itself
/// (`STORAGE_DEVICE_DESCRIPTOR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    /// The device reports removable media.
    pub removable_media: bool,
    /// Its `STORAGE_BUS_TYPE`.
    pub bus: i32,
}

impl Device {
    /// Reads a `STORAGE_DEVICE_DESCRIPTOR` from the bytes
    /// `IOCTL_STORAGE_QUERY_PROPERTY` returned: `RemovableMedia` at byte 10
    /// and `BusType` at bytes 28–31. `None` if the bytes are too short or say
    /// they are.
    pub fn from_descriptor(bytes: &[u8]) -> Option<Self> {
        let size = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
        if usize::try_from(size).ok()? < 32 {
            return None;
        }
        Some(Self {
            removable_media: *bytes.get(10)? != 0,
            bus: i32::from_le_bytes(bytes.get(28..32)?.try_into().ok()?),
        })
    }

    /// Whether the disk can be unplugged and carried away.
    pub fn detachable(self) -> bool {
        self.removable_media || DETACHABLE_BUSES.contains(&self.bus)
    }
}

/// `FILE_PERSISTENT_ACLS` in `GetVolumeInformationW`'s flags.
pub const FILE_PERSISTENT_ACLS: u32 = 0x0000_0008;
/// `FILE_READ_ONLY_VOLUME` in `GetVolumeInformationW`'s flags.
pub const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;

/// What was learned about one mounted drive; `None` where a query failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeFacts {
    /// The drive root, such as `D:\`.
    pub root: String,
    /// What `GetDriveTypeW` says.
    pub drive_type: DriveType,
    /// The storage device under it.
    pub device: Option<Device>,
    /// The filesystem name (`NTFS`, `ReFS`, `exFAT`, …).
    pub filesystem: Option<String>,
    /// The filesystem flags.
    pub flags: Option<u32>,
    /// The 64-bit volume serial number, as file identities carry it
    /// (`FILE_ID_INFO::VolumeSerialNumber`).
    pub serial: Option<u64>,
}

impl VolumeFacts {
    /// Whether this volume, on its own, may hold a store.
    fn eligible(&self) -> bool {
        let filesystem = self
            .filesystem
            .as_deref()
            .is_some_and(|f| f.eq_ignore_ascii_case("NTFS") || f.eq_ignore_ascii_case("ReFS"));
        let flags = self
            .flags
            .is_some_and(|f| f & FILE_PERSISTENT_ACLS != 0 && f & FILE_READ_ONLY_VOLUME == 0);
        self.drive_type == DriveType::Fixed
            && self.device.is_some_and(|d| !d.detachable())
            && filesystem
            && flags
            && self.serial.is_some()
    }
}

/// A volume that may hold a quarantine store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreVolume {
    /// The drive root, such as `D:\`.
    pub root: String,
    /// Its 64-bit volume serial number.
    pub serial: u64,
}

/// The volumes among `facts` that may hold a store. A serial number that
/// more than one mounted volume reports (a cloned disk) disqualifies all of
/// them: identity checks could not tell them apart.
pub fn store_volumes(facts: &[VolumeFacts]) -> Vec<StoreVolume> {
    let shared = |serial: u64| facts.iter().filter(|f| f.serial == Some(serial)).count() > 1;
    facts
        .iter()
        .filter(|f| f.eligible())
        .filter_map(|f| {
            let serial = f.serial?;
            (!shared(serial)).then(|| StoreVolume {
                root: f.root.clone(),
                serial,
            })
        })
        .collect()
}

/// Whether `sid` names a user account that can have a profile: a local or
/// domain account (`S-1-5-21-…`) or a Microsoft Entra account (`S-1-12-1-…`),
/// not a system or service account.
fn is_user_account(sid: &str) -> bool {
    let upper = sid.to_ascii_uppercase();
    upper.starts_with("S-1-5-21-") || upper.starts_with("S-1-12-1-")
}

/// Whether the profile list names exactly one user account, and it is
/// `current` (a SID string).
pub fn single_user(profiles: &[String], current: &str) -> bool {
    let mut users = profiles.iter().filter(|sid| is_user_account(sid));
    matches!(
        (users.next(), users.next()),
        (Some(only), None) if only.eq_ignore_ascii_case(current)
    )
}

/// Every mounted drive letter, with what could be learned about it. Nothing
/// is opened for writing, and a query that fails leaves its fact `None`.
#[cfg(windows)]
pub fn mounted() -> Vec<VolumeFacts> {
    use crate::sys;

    let letters = sys::logical_drives();
    (0..26_u8)
        .filter(|i| letters & (1 << i) != 0)
        .map(|i| {
            let letter = char::from(b'A' + i);
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().collect();
            let handle = sys::open(&wide, sys::Access::Attributes).ok();
            let info = handle.as_ref().and_then(|h| sys::volume_info(h).ok());
            VolumeFacts {
                drive_type: DriveType::from_win32(sys::drive_type(&wide)),
                device: sys::storage_device(letter)
                    .ok()
                    .and_then(|bytes| Device::from_descriptor(&bytes)),
                filesystem: info.as_ref().map(|(name, _)| name.clone()),
                flags: info.map(|(_, flags)| flags),
                serial: handle
                    .as_ref()
                    .and_then(|h| sys::id(h).ok())
                    .map(|id| id.VolumeSerialNumber),
                root,
            }
        })
        .collect()
}

/// Whether the current user is the only user account with a profile on this
/// PC (ADR-0046). `false` if the profile list or the token cannot be read.
#[cfg(windows)]
pub fn single_user_pc() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY};

    const PROFILES: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";
    let Ok(key) = crate::registry::open(HKEY_LOCAL_MACHINE, PROFILES, KEY_WOW64_64KEY) else {
        return false;
    };
    let (Ok(profiles), Ok(current)) = (key.subkeys(), crate::sys::current_user_sid()) else {
        return false;
    };
    single_user(&profiles, &current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(removable: bool, bus: i32) -> Vec<u8> {
        let mut bytes = vec![0_u8; 40];
        bytes[0..4].copy_from_slice(&1_u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&40_u32.to_le_bytes());
        bytes[10] = u8::from(removable);
        bytes[28..32].copy_from_slice(&bus.to_le_bytes());
        bytes
    }

    fn drive(root: &str, serial: u64) -> VolumeFacts {
        VolumeFacts {
            root: root.into(),
            drive_type: DriveType::Fixed,
            // BusTypeNvme
            device: Device::from_descriptor(&descriptor(false, 17)),
            filesystem: Some("NTFS".into()),
            flags: Some(FILE_PERSISTENT_ACLS | 0x0000_0002),
            serial: Some(serial),
        }
    }

    type Change = Box<dyn Fn(&mut VolumeFacts)>;

    fn roots(facts: &[VolumeFacts]) -> Vec<String> {
        store_volumes(facts).into_iter().map(|v| v.root).collect()
    }

    #[test]
    fn descriptors_are_read_at_their_offsets() {
        assert_eq!(
            Device::from_descriptor(&descriptor(true, 7)),
            Some(Device {
                removable_media: true,
                bus: 7
            })
        );
        assert_eq!(Device::from_descriptor(&descriptor(false, 17)[..31]), None);
        let mut short = descriptor(false, 17);
        short[4..8].copy_from_slice(&12_u32.to_le_bytes());
        assert_eq!(Device::from_descriptor(&short), None, "says it is short");
        assert_eq!(Device::from_descriptor(&[]), None);
    }

    #[test]
    fn only_fixed_internal_ntfs_or_refs_with_acls_qualifies() {
        let ok = drive(r"D:\", 1);
        assert_eq!(roots(std::slice::from_ref(&ok)), [r"D:\"]);
        let refs = VolumeFacts {
            filesystem: Some("refs".into()),
            ..drive(r"E:\", 2)
        };
        assert_eq!(roots(&[refs]), [r"E:\"]);
        let changed: Vec<Change> = vec![
            Box::new(|f| f.drive_type = DriveType::Removable),
            Box::new(|f| f.drive_type = DriveType::Remote),
            Box::new(|f| f.drive_type = DriveType::RamDisk),
            Box::new(|f| f.device = Device::from_descriptor(&descriptor(false, 7))),
            Box::new(|f| f.device = Device::from_descriptor(&descriptor(false, 4))),
            Box::new(|f| f.device = Device::from_descriptor(&descriptor(false, 12))),
            Box::new(|f| f.device = Device::from_descriptor(&descriptor(false, 13))),
            Box::new(|f| f.device = Device::from_descriptor(&descriptor(true, 17))),
            Box::new(|f| f.device = None),
            Box::new(|f| f.filesystem = Some("exFAT".into())),
            Box::new(|f| f.filesystem = Some("FAT32".into())),
            Box::new(|f| f.filesystem = None),
            Box::new(|f| f.flags = Some(0x0000_0002)),
            Box::new(|f| f.flags = Some(FILE_PERSISTENT_ACLS | FILE_READ_ONLY_VOLUME)),
            Box::new(|f| f.flags = None),
            Box::new(|f| f.serial = None),
        ];
        for (i, change) in changed.iter().enumerate() {
            let mut facts = ok.clone();
            change(&mut facts);
            assert!(roots(&[facts]).is_empty(), "change {i}");
        }
    }

    #[test]
    fn a_shared_serial_disqualifies_every_volume_with_it() {
        let clone = VolumeFacts {
            drive_type: DriveType::Removable,
            ..drive(r"F:\", 7)
        };
        assert_eq!(
            roots(&[drive(r"C:\", 1), drive(r"D:\", 7), clone]),
            [r"C:\"],
            "D: shares its serial with a removable clone"
        );
        assert_eq!(
            roots(&[drive(r"D:\", 3), drive(r"E:\", 3)]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn single_user_means_one_user_profile_and_it_is_mine() {
        let me = "S-1-5-21-1-2-3-1001";
        let system = ["S-1-5-18", "S-1-5-19", "S-1-5-20"].map(String::from);
        let with = |users: &[&str]| -> Vec<String> {
            system
                .iter()
                .cloned()
                .chain(users.iter().map(|s| (*s).to_owned()))
                .collect()
        };
        assert!(single_user(&with(&[me]), me));
        assert!(single_user(&with(&[&me.to_ascii_lowercase()]), me));
        assert!(!single_user(&with(&[me, "S-1-5-21-1-2-3-1002"]), me));
        assert!(
            !single_user(&with(&[me, "S-1-12-1-9-9-9-9"]), me),
            "an Entra account"
        );
        assert!(
            !single_user(&with(&["S-1-5-21-1-2-3-1002"]), me),
            "not mine"
        );
        assert!(!single_user(&with(&[]), me));
        assert!(single_user(
            &with(&["S-1-12-1-9-9-9-9"]),
            "S-1-12-1-9-9-9-9"
        ));
    }

    #[test]
    fn drive_types_map_from_win32() {
        assert_eq!(DriveType::from_win32(3), DriveType::Fixed);
        assert_eq!(DriveType::from_win32(2), DriveType::Removable);
        assert_eq!(DriveType::from_win32(4), DriveType::Remote);
        assert_eq!(DriveType::from_win32(0), DriveType::Other);
        assert_eq!(DriveType::from_win32(1), DriveType::Other);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn the_system_drive_is_a_fixed_ntfs_volume_that_may_hold_a_store() {
        let facts = mounted();
        let system = facts
            .iter()
            .find(|f| f.root.eq_ignore_ascii_case(r"C:\"))
            .unwrap_or_else(|| unreachable!("C: is mounted: {facts:?}"));
        assert_eq!(system.drive_type, DriveType::Fixed, "{system:?}");
        assert!(system.device.is_some(), "{system:?}");
        assert_eq!(system.filesystem.as_deref(), Some("NTFS"), "{system:?}");
        assert!(
            system.flags.is_some_and(|f| f & FILE_PERSISTENT_ACLS != 0),
            "{system:?}"
        );
        assert!(system.serial.is_some_and(|s| s != 0), "{system:?}");
        assert!(
            store_volumes(&facts).iter().any(|v| v.root == system.root),
            "{facts:?}"
        );
    }

    #[test]
    fn the_profile_list_and_token_can_be_read() {
        // Hosted runners may have more than one profile; this only checks
        // that the answer comes from the registry and token without failing.
        let current = crate::sys::current_user_sid().unwrap_or_default();
        assert!(current.starts_with("S-1-"), "{current}");
        let _single = single_user_pc();
    }
}
