// Windows registry integration — persists global SDK PATH into HKCU\Environment.
// On non-Windows platforms this module compiles to no-ops.

use std::collections::HashMap;

use anyhow::Result;

#[cfg(windows)]
mod win {
    use std::collections::HashMap;
    use anyhow::{Context, Result};
    use winreg::{enums::*, RegKey};

    const ENV_KEY: &str = "Environment";
    const PATH_NAME: &str = "Path";

    pub fn apply(paths: &[String], vars: &HashMap<String, String>) -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(ENV_KEY, KEY_SET_VALUE | KEY_QUERY_VALUE)
            .context("opening HKCU\\Environment")?;

        set_path_value(&key, paths)?;
        set_env_vars(&key, vars)?;
        broadcast_env_change();
        Ok(())
    }

    pub fn remove(paths: &[String], vars: &HashMap<String, String>) -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(ENV_KEY, KEY_SET_VALUE | KEY_QUERY_VALUE)
            .context("opening HKCU\\Environment")?;

        remove_path_value(&key, paths)?;
        delete_env_vars(&key, vars)?;
        broadcast_env_change();
        Ok(())
    }

    fn set_path_value(key: &RegKey, new_paths: &[String]) -> Result<()> {
        if new_paths.is_empty() {
            return Ok(());
        }
        let current: String = key.get_value(PATH_NAME).unwrap_or_default();
        let mut all: Vec<String> = new_paths.to_vec();
        for p in current.split(';').filter(|s| !s.is_empty()) {
            let p = p.to_string();
            if !all.iter().any(|x| x.eq_ignore_ascii_case(&p)) {
                all.push(p);
            }
        }
        key.set_value(PATH_NAME, &all.join(";")).context("setting Path")
    }

    fn remove_path_value(key: &RegKey, rm_paths: &[String]) -> Result<()> {
        if rm_paths.is_empty() {
            return Ok(());
        }
        let current: String = key.get_value(PATH_NAME).unwrap_or_default();
        let remaining: Vec<&str> = current
            .split(';')
            .filter(|p| !p.is_empty() && !rm_paths.iter().any(|r| r.eq_ignore_ascii_case(p)))
            .collect();
        key.set_value(PATH_NAME, &remaining.join(";")).context("setting Path")
    }

    fn set_env_vars(key: &RegKey, vars: &HashMap<String, String>) -> Result<()> {
        for (k, v) in vars {
            key.set_value(k, v).with_context(|| format!("setting {}", k))?;
        }
        Ok(())
    }

    fn delete_env_vars(key: &RegKey, vars: &HashMap<String, String>) -> Result<()> {
        for k in vars.keys() {
            let _ = key.delete_value(k); // Ignore "not found" errors
        }
        Ok(())
    }

    /// Notify all top-level windows that environment has changed.
    fn broadcast_env_change() {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "user32")]
        extern "system" {
            fn SendMessageTimeoutW(
                hwnd:    isize,
                msg:     u32,
                wparam:  usize,
                lparam:  *const u16,
                flags:   u32,
                timeout: u32,
                result:  *mut usize,
            ) -> isize;
        }

        let env_str: Vec<u16> = OsStr::new("Environment")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut result: usize = 0;
        unsafe {
            SendMessageTimeoutW(
                0xffff_u32 as isize, // HWND_BROADCAST
                0x001A,              // WM_SETTINGCHANGE
                0,
                env_str.as_ptr(),
                0x0002,              // SMTO_ABORTIFHUNG
                5000,
                &mut result,
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API (no-op on non-Windows)
// ─────────────────────────────────────────────────────────────────────────────

/// Add `paths` and `vars` to the persistent user environment (HKCU\Environment on Windows).
pub fn apply_global_env(paths: &[String], vars: &HashMap<String, String>) -> Result<()> {
    #[cfg(windows)]
    win::apply(paths, vars)?;
    #[cfg(not(windows))]
    let _ = (paths, vars);
    Ok(())
}

/// Remove `paths` and `vars` from the persistent user environment.
pub fn remove_global_env(paths: &[String], vars: &HashMap<String, String>) -> Result<()> {
    #[cfg(windows)]
    win::remove(paths, vars)?;
    #[cfg(not(windows))]
    let _ = (paths, vars);
    Ok(())
}
