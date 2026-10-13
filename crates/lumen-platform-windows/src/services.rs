//! Windows services and drivers from the Service Control Manager (roadmap 6.11,
//! part 2). The mapping is pure and tested everywhere; the SCM calls are
//! Windows-only.

use lumen_domain::{Service, ServiceKind, ServiceScope, UntrustedText};

use crate::startup::program_of;

/// `SERVICE_KERNEL_DRIVER | SERVICE_FILE_SYSTEM_DRIVER` (`winsvc.h`).
const DRIVER_TYPES: u32 = 0x1 | 0x2;
/// `SERVICE_DISABLED` (`winsvc.h`).
const START_DISABLED: u32 = 4;

/// A service as configured. Drivers' paths are usually relative to the system
/// root (`\SystemRoot\System32\drivers\x.sys`) and then name no program.
pub fn service_entry(name: &str, binary_path: &str, start_type: u32, service_type: u32) -> Service {
    Service {
        kind: if service_type & DRIVER_TYPES != 0 {
            ServiceKind::Driver
        } else {
            ServiceKind::WindowsService
        },
        scope: ServiceScope::System,
        label: UntrustedText::new(name),
        definition: None,
        program: program_of(binary_path),
        // The start type is authoritative: a disabled service cannot start.
        enabled: Some(start_type != START_DISABLED),
        signature: None,
    }
}

#[cfg(windows)]
pub use scm::Services;

#[cfg(windows)]
#[allow(unsafe_code)]
mod scm {
    use lumen_application::CancelToken;
    use lumen_application::ports::{InventoryReport, InventorySource};
    use lumen_domain::{Service, SourceName, SourceStatus};
    use windows_sys::Win32::Foundation::ERROR_MORE_DATA;
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW,
        OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, SC_ENUM_PROCESS_INFO, SC_HANDLE,
        SC_MANAGER_ENUMERATE_SERVICE, SERVICE_DRIVER, SERVICE_QUERY_CONFIG, SERVICE_STATE_ALL,
        SERVICE_WIN32,
    };

    use super::service_entry;
    use crate::winpath::nul_terminated;

    const SOURCE: &str = "windows.services";

    /// A Service Control Manager handle, closed on drop.
    struct Handle(SC_HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: invariant: the handle is open and owned by `self`. Why
            // unsafe: FFI call. Why it holds: only successful opens build a
            // `Handle`, and nothing else closes it.
            let closed = unsafe { CloseServiceHandle(self.0) };
            if closed == 0 {
                // Nothing useful can be done in `drop`.
            }
        }
    }

    /// A pointer-aligned byte buffer that the SCM fills with structs and the
    /// strings they point to.
    struct Buffer(Vec<u64>);

    impl Buffer {
        fn new(bytes: usize) -> Self {
            Self(vec![0; bytes.div_ceil(8)])
        }

        fn len_bytes(&self) -> usize {
            self.0.len() * 8
        }

        /// Reads the NUL-terminated UTF-16 string at `ptr` if it lies inside the
        /// buffer, never reading past the buffer's end.
        fn string(&self, ptr: *const u16) -> Option<String> {
            let start = self.0.as_ptr() as usize;
            let at = ptr as usize;
            let end = start + self.len_bytes();
            if ptr.is_null() || at < start || at >= end || !(at - start).is_multiple_of(2) {
                return None;
            }
            let units = (end - at) / 2;
            // SAFETY: invariant: `ptr..ptr+units` lies inside the buffer. Why
            // unsafe: building a slice from a raw pointer. Why it holds: the
            // bounds and alignment were checked just above, and the buffer is
            // borrowed for the slice's lifetime.
            let all = unsafe { std::slice::from_raw_parts(ptr, units) };
            let len = all.iter().position(|u| *u == 0)?;
            Some(String::from_utf16_lossy(&all[..len]))
        }
    }

    /// Every service name, or `None` if the list cannot be read completely.
    fn names(manager: &Handle) -> Option<Vec<String>> {
        let mut names = Vec::new();
        let mut resume = 0_u32;
        let mut buffer = Buffer::new(64 * 1024);
        loop {
            let mut needed = 0_u32;
            let mut count = 0_u32;
            let size = u32::try_from(buffer.len_bytes()).ok()?;
            // SAFETY: invariant: the SCM writes at most `size` bytes into the
            // buffer, plus the counters into locals. Why unsafe: FFI writing
            // through raw pointers. Why it holds: `size` is the buffer's size and
            // all out-pointers are to live locals; the group name may be null.
            let ok = unsafe {
                EnumServicesStatusExW(
                    manager.0,
                    SC_ENUM_PROCESS_INFO,
                    SERVICE_WIN32 | SERVICE_DRIVER,
                    SERVICE_STATE_ALL,
                    buffer.0.as_mut_ptr().cast(),
                    size,
                    &raw mut needed,
                    &raw mut count,
                    &raw mut resume,
                    std::ptr::null(),
                )
            };
            let more = ok == 0
                && std::io::Error::last_os_error().raw_os_error()
                    == i32::try_from(ERROR_MORE_DATA).ok();
            if ok == 0 && !more {
                return None;
            }
            let count = usize::try_from(count).ok()?;
            let max = buffer.len_bytes() / size_of::<ENUM_SERVICE_STATUS_PROCESSW>();
            let first = buffer.0.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
            for index in 0..count.min(max) {
                // SAFETY: invariant: the first `count` records are initialised
                // structs inside the buffer. Why unsafe: reading through a raw
                // pointer. Why it holds: the SCM reported `count` records, the
                // index is bounded by the buffer's capacity, and the buffer is
                // 8-byte aligned like the struct.
                let record = unsafe { first.add(index).read() };
                names.extend(buffer.string(record.lpServiceName));
            }
            if !more {
                return Some(names);
            }
            if count == 0 {
                buffer = Buffer::new(usize::try_from(needed).ok()?.max(buffer.len_bytes() * 2));
            }
        }
    }

    /// The configured service, or `None` if its configuration cannot be read.
    fn config(manager: &Handle, name: &str) -> Option<Service> {
        let wide = nul_terminated(&name.encode_utf16().collect::<Vec<_>>())?;
        // SAFETY: invariant: `wide` is NUL-terminated and outlives the call. Why
        // unsafe: FFI call. Why it holds: a local `Vec` with a pushed NUL.
        let raw = unsafe { OpenServiceW(manager.0, wide.as_ptr(), SERVICE_QUERY_CONFIG) };
        if raw.is_null() {
            return None;
        }
        let service = Handle(raw);
        let mut buffer = Buffer::new(8 * 1024);
        for _ in 0..2 {
            let mut needed = 0_u32;
            let size = u32::try_from(buffer.len_bytes()).ok()?;
            // SAFETY: invariant: the SCM writes at most `size` bytes into the
            // buffer. Why unsafe: FFI writing through a raw pointer. Why it holds:
            // `size` is the buffer's size and `needed` is a live local.
            let ok = unsafe {
                QueryServiceConfigW(
                    service.0,
                    buffer.0.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>(),
                    size,
                    &raw mut needed,
                )
            };
            if ok != 0 {
                // SAFETY: invariant: the buffer starts with an initialised
                // config struct. Why unsafe: reading through a raw pointer. Why it
                // holds: the call succeeded, and the buffer is aligned and larger
                // than the struct.
                let cfg = unsafe { buffer.0.as_ptr().cast::<QUERY_SERVICE_CONFIGW>().read() };
                let path = buffer.string(cfg.lpBinaryPathName).unwrap_or_default();
                return Some(service_entry(
                    name,
                    &path,
                    cfg.dwStartType,
                    cfg.dwServiceType,
                ));
            }
            buffer = Buffer::new(usize::try_from(needed).ok()?.max(buffer.len_bytes()));
        }
        None
    }

    /// Lists Win32 services and drivers.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Services;

    fn source() -> SourceName {
        SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("valid source name"))
    }

    impl InventorySource for Services {
        fn name(&self) -> SourceName {
            source()
        }

        fn collect(&self, cancel: &CancelToken) -> InventoryReport {
            let report = |services, complete| InventoryReport {
                source: source(),
                status: if complete {
                    SourceStatus::Complete
                } else {
                    SourceStatus::Partial
                },
                applications: Vec::new(),
                services,
                packages: Vec::new(),
            };
            // SAFETY: invariant: null machine and database names select the local
            // active database. Why unsafe: FFI call. Why it holds: documented.
            let raw = unsafe {
                OpenSCManagerW(
                    std::ptr::null(),
                    std::ptr::null(),
                    SC_MANAGER_ENUMERATE_SERVICE,
                )
            };
            if raw.is_null() {
                return InventoryReport {
                    status: SourceStatus::Unavailable,
                    ..report(Vec::new(), false)
                };
            }
            let manager = Handle(raw);
            let Some(names) = names(&manager) else {
                return report(Vec::new(), false);
            };
            let mut services = Vec::with_capacity(names.len());
            let mut complete = true;
            for name in names {
                if cancel.is_cancelled() {
                    complete = false;
                    break;
                }
                match config(&manager, &name) {
                    Some(service) => services.push(service),
                    // Denied or vanished: unknown, not absent.
                    None => complete = false,
                }
            }
            report(services, complete)
        }
    }

    #[cfg(test)]
    mod tests {
        use lumen_domain::ServiceKind;

        use super::*;

        #[test]
        fn lists_services_and_drivers() {
            let report = Services.collect(&CancelToken::new());
            assert_eq!(report.source.as_str(), SOURCE);
            let has = |label: &str| {
                report
                    .services
                    .iter()
                    .find(|s| s.label.raw().eq_ignore_ascii_case(label))
            };
            // Present on every Windows: the Event Log service and the NTFS driver.
            assert_eq!(
                has("EventLog").map(|s| s.kind),
                Some(ServiceKind::WindowsService)
            );
            assert_eq!(has("Ntfs").map(|s| s.kind), Some(ServiceKind::Driver));
            assert!(
                has("EventLog").and_then(|s| s.program.as_ref()).is_some(),
                "svchost.exe path"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_map_kind_scope_program_and_start_type() {
        let svc = service_entry(
            "EventLog",
            r"C:\Windows\System32\svchost.exe -k LocalServiceNetworkRestricted -p",
            2,
            0x20,
        );
        assert_eq!(svc.kind, ServiceKind::WindowsService);
        assert_eq!(svc.scope, ServiceScope::System);
        assert_eq!(svc.enabled, Some(true));
        let expected: Vec<u16> = r"C:\Windows\System32\svchost.exe".encode_utf16().collect();
        assert_eq!(
            svc.program.and_then(|p| p.to_windows_wide()),
            Some(expected)
        );

        let driver = service_entry("Ntfs", r"\SystemRoot\System32\drivers\ntfs.sys", 0, 0x2);
        assert_eq!(driver.kind, ServiceKind::Driver);
        assert_eq!(driver.program, None, "relative to the system root");

        let disabled = service_entry("Old", r#""C:\Old\svc.exe""#, START_DISABLED, 0x10);
        assert_eq!(disabled.enabled, Some(false));
    }
}
