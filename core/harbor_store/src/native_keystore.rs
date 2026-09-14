//! OS keystore adapters for the device root key.
//!
//! [`KeyStore`](crate::keys::KeyStore) implementations backed by real
//! OS-protected storage, one per platform:
//!
//! - **macOS / iOS** — [`KeychainKeyStore`]: a generic-password item in
//!   the platform keychain (Security framework). The 32-byte root never
//!   touches the filesystem.
//! - **Windows** — [`DpapiKeyStore`]: the root is sealed with DPAPI
//!   (`CryptProtectData`, bound to the user's Windows credential) in the
//!   app-private keys directory.
//! - **Android** — the NDK side of a `dlopen`ed library has no JavaVM, so
//!   Rust cannot reach `AndroidKeyStore` directly. The app's Kotlin
//!   embedding generates a 32-byte root, seals it under a non-exportable
//!   AndroidKeyStore AES-GCM key (TEE/StrongBox), and injects the
//!   unwrapped root at open time via [`InjectedKeyStore`]
//!   (`harbor_core_open_ex`). At rest only the Keystore-sealed blob
//!   exists.
//! - **Development / tests** — `FileKeyStore` (raw file, 0600). Never
//!   selected by the production FFI open path on the platforms above.

use crate::error::{Result, StoreError};
use crate::keys::{KeyMaterial, KeyStore};

/// Root key supplied by the embedding layer (Android Keystore unseal
/// path). The key was generated inside the app and is sealed at rest by
/// the OS keystore; this adapter only hands it to the workspace.
pub struct InjectedKeyStore {
    root: KeyMaterial,
}

impl InjectedKeyStore {
    pub fn new(root: KeyMaterial) -> Self {
        InjectedKeyStore { root }
    }
}

impl KeyStore for InjectedKeyStore {
    fn device_root_key(&self, _service: &str) -> Result<KeyMaterial> {
        Ok(self.root.clone())
    }
}

/// Keychain-backed store (macOS + iOS). One generic-password item per
/// service under a shared Harbor service namespace.
#[derive(Debug, Clone)]
pub struct KeychainKeyStore {
    service_namespace: String,
}

impl KeychainKeyStore {
    pub fn new() -> Result<Self> {
        Ok(KeychainKeyStore {
            service_namespace: "dev.harbor.core".to_string(),
        })
    }

    /// Item account name for a Harbor service ("namespace/service").
    fn account(&self, service: &str) -> String {
        format!("{}/{}", self.service_namespace, service)
    }
}

impl Default for KeychainKeyStore {
    fn default() -> Self {
        KeychainKeyStore {
            service_namespace: "dev.harbor.core".to_string(),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod imp {
    use super::*;
    use security_framework::passwords;

    impl KeyStore for KeychainKeyStore {
        fn device_root_key(&self, service: &str) -> Result<KeyMaterial> {
            let account = self.account(service);
            match passwords::get_generic_password(&self.service_namespace, &account) {
                Ok(bytes) => KeyMaterial::from_bytes(&bytes),
                Err(_) => {
                    // First use on this device: generate and store.
                    let key = KeyMaterial::random();
                    passwords::set_generic_password(
                        &self.service_namespace,
                        &account,
                        &key.0,
                    )
                    .map_err(|_| StoreError::Crypto)?;
                    Ok(key)
                }
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
mod imp {
    use super::*;
    impl KeyStore for KeychainKeyStore {
        fn device_root_key(&self, _service: &str) -> Result<KeyMaterial> {
            Err(StoreError::Crypto)
        }
    }
}

pub use imp::*;

/// DPAPI-backed store (Windows). The root key is stored
/// `CryptProtectData`-sealed (user scope) in the keys directory.
#[cfg(windows)]
pub struct DpapiKeyStore {
    dir: std::path::PathBuf,
}

#[cfg(windows)]
mod dpapi {
    use super::*;

    #[repr(C)]
    struct CryptBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            data_in: *const CryptBlob,
            data_descr: *const u16,
            optional_entropy: *const CryptBlob,
            reserved: *mut core::ffi::c_void,
            prompt_struct: *mut core::ffi::c_void,
            flags: u32,
            data_out: *mut CryptBlob,
        ) -> i32;
        fn CryptUnprotectData(
            data_in: *const CryptBlob,
            ppsz_data_descr: *mut *mut u16,
            optional_entropy: *const CryptBlob,
            reserved: *mut core::ffi::c_void,
            prompt_struct: *mut core::ffi::c_void,
            flags: u32,
            data_out: *mut CryptBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(hmem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut out = CryptBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
        let input = CryptBlob {
            cb_data: plaintext.len() as u32,
            pb_data: plaintext.as_ptr() as *mut u8,
        };
        let descr: Vec<u16> = "harbor device root key\0".encode_utf16().collect();
        // SAFETY: blobs point at valid buffers; output freed below.
        let ok = unsafe {
            CryptProtectData(
                &input,
                descr.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(StoreError::Crypto);
        }
        let sealed =
            unsafe { std::slice::from_raw_parts(out.pb_data, out.cb_data as usize) }.to_vec();
        unsafe { LocalFree(out.pb_data as *mut _) };
        Ok(sealed)
    }

    pub fn unprotect(sealed: &[u8]) -> Result<Vec<u8>> {
        let mut out = CryptBlob { cb_data: 0, pb_data: std::ptr::null_mut() };
        let input = CryptBlob {
            cb_data: sealed.len() as u32,
            pb_data: sealed.as_ptr() as *mut u8,
        };
        // SAFETY: blobs point at valid buffers; output freed below.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(StoreError::Crypto);
        }
        let plain =
            unsafe { std::slice::from_raw_parts(out.pb_data, out.cb_data as usize) }.to_vec();
        unsafe { LocalFree(out.pb_data as *mut _) };
        Ok(plain)
    }
}

#[cfg(windows)]
impl DpapiKeyStore {
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(DpapiKeyStore { dir })
    }

    fn key_path(&self, service: &str) -> std::path::PathBuf {
        self.dir.join(format!(
            "{}.dpapi",
            service.replace(['/', '\\', ':'], "_")
        ))
    }
}

#[cfg(windows)]
impl KeyStore for DpapiKeyStore {
    fn device_root_key(&self, service: &str) -> Result<KeyMaterial> {
        let path = self.key_path(service);
        if path.exists() {
            let sealed = std::fs::read(&path)?;
            let plain = dpapi::unprotect(&sealed)?;
            return KeyMaterial::from_bytes(&plain);
        }
        let key = KeyMaterial::random();
        let sealed = dpapi::protect(&key.0)?;
        std::fs::write(&path, &sealed)?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keychain adapter is the production root-key source on this
    /// platform: a generated root must persist across adapter instances
    /// and stay out of the filesystem. The item is deleted afterwards so
    /// the test leaves no residue in the user keychain.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[test]
    fn keychain_root_key_persists_across_instances() {
        use security_framework::passwords;
        let ks = KeychainKeyStore::new().unwrap();
        let unique = format!("harbor.test.{}", crate::keys::KeyMaterial::random().0[0]);
        let k1 = ks.device_root_key(&unique).unwrap();
        let ks2 = KeychainKeyStore::new().unwrap();
        let k2 = ks2.device_root_key(&unique).unwrap();
        assert_eq!(k1, k2, "same service must return the persisted keychain key");
        let other = ks.device_root_key(&format!("{unique}.other")).unwrap();
        assert_ne!(k1, other);
        let _ = passwords::delete_generic_password(
            &ks.service_namespace,
            &ks.account(&unique),
        );
        let _ = passwords::delete_generic_password(
            &ks.service_namespace,
            &ks.account(&format!("{unique}.other")),
        );
    }

    #[test]
    fn injected_keystore_returns_the_injected_root() {
        let root = KeyMaterial::random();
        let ks = InjectedKeyStore::new(root.clone());
        assert_eq!(ks.device_root_key("harbor.device").unwrap(), root);
    }
}
