//! Cache one public intermediate, never a trust anchor or a browser TLS exception.
//! NSS comes from the same installed Firefox; only a CEH-owned, leased profile is opened.
use crate::firefox::{FirefoxError, FirefoxInstallation, ProfileLease};
const INTERMEDIATE: &[u8] = include_bytes!("../assets/rapidssl-tls-rsa-ca-g1.der");
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
static NSS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[repr(C)]
#[derive(Default)]
struct CertTrust {
    ssl: u32,
    email: u32,
    object_signing: u32,
}
#[cfg(target_os = "macos")]
pub(crate) fn prepare(
    profile: &ProfileLease,
    installation: &FirefoxInstallation,
) -> Result<usize, FirefoxError> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let _lock = NSS_LOCK.lock().map_err(|_| FirefoxError::Unavailable)?;
    let folder = installation
        .executable()
        .parent()
        .ok_or(FirefoxError::InvalidInstallation)?;
    let profile_name = CString::new([b"sql:", profile.path().as_os_str().as_bytes()].concat())
        .map_err(|_| FirefoxError::InvalidProfile)?;
    // Library names/locations belong to the validated installation, never to a UI path.
    unsafe {
        let glue = libloading::os::unix::Library::open(
            Some(folder.join("libmozglue.dylib")),
            libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL,
        )
        .map_err(|_| FirefoxError::Unavailable)?;
        let nss = libloading::Library::new(folder.join("libnss3.dylib"))
            .map_err(|_| FirefoxError::Unavailable)?;
        let result = import_intermediate(&nss, &profile_name);
        drop(nss);
        drop(glue);
        result
    }
}
/// Windows Firefox does not fetch a missing intermediate either, and a new profile has no preloaded copy:
/// without this the site opens on "Warning: Security Risk" and the search cannot start.
#[cfg(target_os = "windows")]
pub(crate) fn prepare(
    profile: &ProfileLease,
    installation: &FirefoxInstallation,
) -> Result<usize, FirefoxError> {
    use std::ffi::CString;
    let _lock = NSS_LOCK.lock().map_err(|_| FirefoxError::Unavailable)?;
    let folder = installation
        .executable()
        .parent()
        .ok_or(FirefoxError::InvalidInstallation)?;
    let profile_name = CString::new(format!("sql:{}", nss_profile_path(profile.path())?))
        .map_err(|_| FirefoxError::InvalidProfile)?;
    // nss3.dll needs mozglue.dll and its other neighbours: resolve them from the installation folder.
    unsafe {
        let nss = libloading::os::windows::Library::load_with_flags(
            folder.join("nss3.dll"),
            libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
        )
        .map_err(|_| FirefoxError::Unavailable)?;
        let nss = libloading::Library::from(nss);
        let result = import_intermediate(&nss, &profile_name);
        drop(nss);
        result
    }
}
/// NSS takes a UTF-8 folder name without the `\\?\` prefix that canonical Windows paths carry.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn nss_profile_path(path: &std::path::Path) -> Result<String, FirefoxError> {
    let text = path.to_str().ok_or(FirefoxError::InvalidProfile)?;
    let text = match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.starts_with(r"UNC\") => format!(r"\\{}", &rest[4..]),
        Some(rest) => rest.to_owned(),
        None => text.to_owned(),
    };
    Ok(text)
}
/// Opens the leased profile's certificate database with the installation's NSS and adds the intermediate
/// without trust bits. `Ok(1)` when it is in the database afterwards.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
unsafe fn import_intermediate(
    nss: &libloading::Library,
    profile_name: &std::ffi::CStr,
) -> Result<usize, FirefoxError> {
    use std::ffi::{c_char, c_int, c_ulong, c_void};
    unsafe {
        let init: libloading::Symbol<unsafe extern "C" fn(*const c_char) -> c_int> = nss
            .get(b"NSS_InitReadWrite\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let shutdown: libloading::Symbol<unsafe extern "C" fn() -> c_int> = nss
            .get(b"NSS_Shutdown\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let get_slot: libloading::Symbol<unsafe extern "C" fn() -> *mut c_void> = nss
            .get(b"PK11_GetInternalKeySlot\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let free_slot: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = nss
            .get(b"PK11_FreeSlot\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let decode: libloading::Symbol<unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void> =
            nss.get(b"CERT_DecodeCertFromPackage\0")
                .map_err(|_| FirefoxError::Unavailable)?;
        let destroy: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = nss
            .get(b"CERT_DestroyCertificate\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        // CK_OBJECT_HANDLE is an `unsigned long`: 32 bits on Windows, 64 on macOS, as `c_ulong` is.
        let import: libloading::Symbol<
            unsafe extern "C" fn(*mut c_void, *mut c_void, c_ulong, *const c_char, c_int) -> c_int,
        > = nss
            .get(b"PK11_ImportCert\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        let get_trust: libloading::Symbol<
            unsafe extern "C" fn(*mut c_void, *mut CertTrust) -> c_int,
        > = nss
            .get(b"CERT_GetCertTrust\0")
            .map_err(|_| FirefoxError::Unavailable)?;
        if init(profile_name.as_ptr()) != 0 {
            return Err(FirefoxError::InvalidProfile);
        }
        let slot = get_slot();
        let result = if slot.is_null() {
            Err(FirefoxError::Unavailable)
        } else {
            let certificate = decode(INTERMEDIATE.as_ptr().cast(), INTERMEDIATE.len() as c_int);
            let result = if certificate.is_null() {
                Err(FirefoxError::Unavailable)
            } else {
                let nickname = c"ceh-intermediate-rapidssl-tls-rsa-ca-g1";
                let status = import(slot, certificate, 0, nickname.as_ptr(), 0); // includeTrust=false
                let mut trust = CertTrust::default();
                let trust_status = get_trust(certificate, &mut trust);
                // Missing trust is expected; if a record exists, no anchor/peer trust bits are allowed.
                let forbidden = 0x02 | 0x10 | 0x20 | 0x80;
                let result = if status == 0
                    && (trust_status != 0
                        || ((trust.ssl | trust.email | trust.object_signing) & forbidden) == 0)
                {
                    Ok(1)
                } else {
                    Err(FirefoxError::Unavailable)
                };
                destroy(certificate);
                result
            };
            free_slot(slot);
            result
        };
        if shutdown() != 0 {
            return Err(FirefoxError::Unavailable);
        }
        result
    }
}
/// Linux Firefox does not fetch the missing intermediate either. Mozilla's own build carries NSS next to the
/// executable; a distribution's package uses the system NSS, which opens the same `cert9.db` profile format.
#[cfg(target_os = "linux")]
pub(crate) fn prepare(
    profile: &ProfileLease,
    installation: &FirefoxInstallation,
) -> Result<usize, FirefoxError> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let _lock = NSS_LOCK.lock().map_err(|_| FirefoxError::Unavailable)?;
    let folder = installation
        .executable()
        .parent()
        .ok_or(FirefoxError::InvalidInstallation)?;
    let profile_name = CString::new([b"sql:", profile.path().as_os_str().as_bytes()].concat())
        .map_err(|_| FirefoxError::InvalidProfile)?;
    unsafe {
        // Mozilla's own build has one combined libnss3 that needs its mozglue loaded first, as on macOS.
        let glue = if folder.join("libmozglue.so").is_file() {
            Some(
                libloading::os::unix::Library::open(
                    Some(folder.join("libmozglue.so")),
                    libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL,
                )
                .map_err(|_| FirefoxError::Unavailable)?,
            )
        } else {
            None
        };
        // A distribution splits NSS into several libraries, and `CERT_DecodeCertFromPackage` lives in libsmime3,
        // which brings libnss3 with it: one handle then reaches every function used here. Debian keeps that set
        // next to Firefox; Arch and Fedora use the system's.
        let nss = [folder.join("libnss3.so"), folder.join("libsmime3.so"), "libsmime3.so".into()]
            .into_iter()
            .filter(|path| path.is_file() || path.as_os_str() == "libsmime3.so")
            .filter_map(|path| libloading::Library::new(&path).ok())
            .find(|library| library.get::<unsafe extern "C" fn()>(b"CERT_DecodeCertFromPackage\0").is_ok())
            .ok_or(FirefoxError::Unavailable)?;
        let result = import_intermediate(&nss, &profile_name);
        drop(nss);
        drop(glue);
        result
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub(crate) fn prepare(
    _profile: &ProfileLease,
    _installation: &FirefoxInstallation,
) -> Result<usize, FirefoxError> {
    let _ = INTERMEDIATE;
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::nss_profile_path;
    use std::path::Path;
    #[test]
    fn nss_gets_the_profile_folder_without_the_verbatim_prefix() {
        assert_eq!(
            nss_profile_path(Path::new(r"\\?\C:\Users\user\AppData\Local\SantaCita\browser-profiles\firefox-a")).unwrap(),
            r"C:\Users\user\AppData\Local\SantaCita\browser-profiles\firefox-a"
        );
        assert_eq!(nss_profile_path(Path::new(r"\\?\UNC\server\share\p")).unwrap(), r"\\server\share\p");
        assert_eq!(nss_profile_path(Path::new(r"C:\p")).unwrap(), r"C:\p");
    }
    /// The ICP site sends its certificate without the RapidSSL intermediate. Firefox does not fetch it, and a new
    /// profile has no preloaded copy, so without this the first page is "Warning: Security Risk" (Windows, 0.4.2).
    /// Starts the installed Firefox the way a search does and checks the intermediate went into its profile.
    #[test]
    #[ignore = "launches the installed Firefox"]
    fn firefox_profile_gets_the_site_intermediate_before_the_first_page() {
        use crate::firefox::{CertificateCache, FirefoxInstallation, OwnedFirefox, ProfileLease};
        let Ok(installation) = FirefoxInstallation::discover() else { return crate::skip_missing_browser("Firefox") };
        let runtime = tempfile::tempdir().unwrap();
        let lease = ProfileLease::temporary(runtime.path()).unwrap();
        let mut browser = OwnedFirefox::start(&installation, lease, true, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        let ready = browser.certificate_cache() == CertificateCache::Ready(1);
        let _ = browser.stop();
        // No state value in the message: it only says whether the profile has the intermediate.
        assert!(ready, "the site's intermediate was not added to the new profile");
    }
}
