//! Secrets in the Windows Credential Manager instead of plain settings files.

/// A saved login: a non-secret `user` value (for example an API URL) and its secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    pub user: String,
    pub secret: String,
}

#[cfg(windows)]
mod platform {
    use super::Credential;
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_error() -> String {
        std::io::Error::last_os_error().to_string()
    }

    pub fn save(target: &str, credential: &Credential) -> Result<(), String> {
        let mut target = wide(target);
        let mut user = wide(&credential.user);
        let mut secret = credential.secret.as_bytes().to_vec();
        let record = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: u32::try_from(secret.len()).map_err(|e| e.to_string())?,
            CredentialBlob: secret.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: user.as_mut_ptr(),
            ..Default::default()
        };
        // SAFETY: every pointer refers to a live buffer owned by this function.
        let written = unsafe { CredWriteW(&record, 0) } != 0;
        secret.fill(0);
        written.then_some(()).ok_or_else(last_error)
    }

    pub fn load(target: &str) -> Option<Credential> {
        let target = wide(target);
        let mut record = std::ptr::null_mut::<CREDENTIALW>();
        // SAFETY: CredReadW allocates `record`, which is released with CredFree below.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut record) } == 0
            || record.is_null()
        {
            return None;
        }
        let credential = unsafe {
            let value = &*record;
            let secret = if value.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(value.CredentialBlob, value.CredentialBlobSize as usize)
                    .to_vec()
            };
            let user = if value.UserName.is_null() {
                String::new()
            } else {
                let length = (0..).take_while(|&i| *value.UserName.add(i) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(value.UserName, length))
            };
            CredFree(record.cast());
            String::from_utf8(secret)
                .ok()
                .map(|secret| Credential { user, secret })
        };
        credential
    }

    pub fn delete(target: &str) -> Result<(), String> {
        let target = wide(target);
        // SAFETY: `target` is a NUL-terminated UTF-16 string.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        // ERROR_NOT_FOUND: nothing was saved, which is the requested end state.
        if error.raw_os_error() == Some(1168) {
            Ok(())
        } else {
            Err(error.to_string())
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Credential;

    pub fn save(_: &str, _: &Credential) -> Result<(), String> {
        Err("No secure credential store is available on this system.".into())
    }

    pub fn load(_: &str) -> Option<Credential> {
        None
    }

    pub fn delete(_: &str) -> Result<(), String> {
        Ok(())
    }
}

pub use platform::{delete, load, save};

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn credentials_round_trip_through_the_windows_store() {
        let target = format!("ADBHelper/Test/{}", std::process::id());
        let credential = Credential {
            user: "https://example.supabase.co/rest/v1/guild_states?select=state".into(),
            secret: "sb_publishable_ÄÖÜ-test".into(),
        };
        save(&target, &credential).unwrap();
        assert_eq!(load(&target), Some(credential));
        delete(&target).unwrap();
        assert_eq!(load(&target), None);
        delete(&target).unwrap();
    }
}
