use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

pub const PROFILE_ENV_VAR: &str = "PEBBLE_PROFILE_DIR";
pub const PROFILE_POINTER_FILENAME: &str = "pebble-profile.txt";
pub const PORTABLE_PROFILE_DIRNAME: &str = "pebble-profile";
const PROFILE_ID_FILENAME: &str = "profile-id";

#[derive(Clone)]
pub struct ProfilePaths {
    app_data_dir: PathBuf,
    storage_namespace: String,
}

impl ProfilePaths {
    /// Fails when the profile id cannot be read or written: continuing with a
    /// path-derived namespace would silently merge the settings of every user
    /// who points at the same (e.g. read-only) directory.
    pub fn new(app_data_dir: PathBuf) -> std::io::Result<Self> {
        let id = load_or_create_profile_id(&app_data_dir)?;
        Ok(Self {
            app_data_dir,
            storage_namespace: profile_storage_namespace(&id),
        })
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    pub fn storage_namespace(&self) -> &str {
        &self.storage_namespace
    }
}

/// The storage namespace is derived from a persistent profile id stored inside
/// the profile directory, so moving or renaming the directory keeps the same
/// frontend storage keys. Returns an error when the id file cannot be read or
/// written (e.g. a read-only profile directory) instead of silently degrading.
fn load_or_create_profile_id(app_data_dir: &Path) -> std::io::Result<String> {
    let id_path = app_data_dir.join(PROFILE_ID_FILENAME);
    match std::fs::read_to_string(&id_path) {
        Ok(existing) => {
            let id = existing.trim().trim_start_matches('\u{feff}');
            if !id.is_empty() {
                return Ok(id.to_string());
            }
            let id = crate::core::new_id();
            std::fs::write(&id_path, &id)?;
            Ok(id)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let id = crate::core::new_id();
            std::fs::write(&id_path, &id)?;
            Ok(id)
        }
        Err(e) => Err(e),
    }
}

fn resolve_profile_path(raw_path: &str, base_dir: &Path) -> Result<PathBuf, String> {
    let trimmed = raw_path.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() {
        return Err("Profile directory path is empty".to_string());
    }

    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(base_dir.join(path))
    }
}

pub fn resolve_profile_data_dir(
    exe_dir: &Path,
    fallback_app_data: PathBuf,
    env_profile_dir: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(raw_path) = env_profile_dir {
        if !raw_path.trim().trim_start_matches('\u{feff}').is_empty() {
            return resolve_profile_path(raw_path, exe_dir);
        }
    }

    let pointer_path = exe_dir.join(PROFILE_POINTER_FILENAME);
    if pointer_path.is_file() {
        let raw_path = std::fs::read_to_string(&pointer_path).map_err(|e| {
            format!(
                "Failed to read profile pointer file {}: {e}",
                pointer_path.display()
            )
        })?;
        // An empty or whitespace-only pointer file is treated as "not
        // configured" and falls through to the portable dir / default,
        // instead of aborting startup.
        let first_line = raw_path.lines().next().unwrap_or_default();
        if !first_line.trim().trim_start_matches('\u{feff}').is_empty() {
            return resolve_profile_path(first_line, exe_dir);
        }
    }

    let portable_path = exe_dir.join(PORTABLE_PROFILE_DIRNAME);
    if portable_path.is_dir() {
        return Ok(portable_path);
    }

    Ok(fallback_app_data)
}

pub fn resolve_app_profile_data_dir(
    app: &tauri::App,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let fallback_app_data = app.path().app_data_dir()?;
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe
        .parent()
        .ok_or("Failed to resolve Pebble executable directory")?;
    let env_profile_dir = std::env::var(PROFILE_ENV_VAR).ok();

    resolve_profile_data_dir(exe_dir, fallback_app_data, env_profile_dir.as_deref())
        .map_err(Into::into)
}

pub fn managed_app_data_dir<R: Runtime, M: Manager<R>>(manager: &M) -> Option<PathBuf> {
    manager
        .try_state::<ProfilePaths>()
        .map(|paths| paths.app_data_dir().to_path_buf())
}

pub fn require_managed_app_data_dir<R: Runtime, M: Manager<R>>(
    manager: &M,
) -> Result<PathBuf, String> {
    managed_app_data_dir(manager).ok_or_else(|| "Profile directory is not initialized".to_string())
}

pub fn profile_storage_namespace(profile_id: &str) -> String {
    let normalized = profile_id.replace('\\', "/").to_ascii_lowercase();
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[tauri::command]
pub fn get_profile_storage_namespace<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let paths = app
        .try_state::<ProfilePaths>()
        .ok_or_else(|| "Profile directory is not initialized".to_string())?;
    Ok(paths.storage_namespace().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be available")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pebble-profile-test-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory should be writable");
        path
    }

    #[test]
    fn env_profile_dir_overrides_default_app_data_dir() {
        let exe_dir = temp_dir("env-exe");
        let fallback = temp_dir("env-fallback");
        let custom = temp_dir("env-custom");

        let resolved =
            resolve_profile_data_dir(&exe_dir, fallback, Some(custom.to_str().unwrap())).unwrap();

        assert_eq!(resolved, custom);
        let _ = fs::remove_dir_all(exe_dir);
        let _ = fs::remove_dir_all(resolved);
    }

    #[test]
    fn pointer_file_can_select_profile_dir_relative_to_exe_dir() {
        let exe_dir = temp_dir("pointer-exe");
        let fallback = temp_dir("pointer-fallback");
        fs::write(
            exe_dir.join(PROFILE_POINTER_FILENAME),
            "profiles/work-profile\n",
        )
        .expect("pointer file should be writable");

        let resolved = resolve_profile_data_dir(&exe_dir, fallback.clone(), None).unwrap();

        assert_eq!(resolved, exe_dir.join("profiles").join("work-profile"));
        let _ = fs::remove_dir_all(exe_dir);
        let _ = fs::remove_dir_all(fallback);
    }

    #[test]
    fn pointer_file_uses_only_first_line_as_profile_dir() {
        let exe_dir = temp_dir("pointer-first-line-exe");
        let fallback = temp_dir("pointer-first-line-fallback");
        fs::write(
            exe_dir.join(PROFILE_POINTER_FILENAME),
            "profiles/work-profile\n# comments can live below the first line\n",
        )
        .expect("pointer file should be writable");

        let resolved = resolve_profile_data_dir(&exe_dir, fallback.clone(), None).unwrap();

        assert_eq!(resolved, exe_dir.join("profiles").join("work-profile"));
        let _ = fs::remove_dir_all(exe_dir);
        let _ = fs::remove_dir_all(fallback);
    }

    #[test]
    fn pointer_file_with_utf8_bom_resolves_the_path() {
        let exe_dir = temp_dir("pointer-bom-exe");
        let fallback = temp_dir("pointer-bom-fallback");
        // Windows Notepad saves UTF-8 with a BOM by default.
        fs::write(
            exe_dir.join(PROFILE_POINTER_FILENAME),
            "\u{feff}profiles/work-profile\n",
        )
        .expect("pointer file should be writable");

        let resolved = resolve_profile_data_dir(&exe_dir, fallback.clone(), None).unwrap();

        assert_eq!(resolved, exe_dir.join("profiles").join("work-profile"));
        let _ = fs::remove_dir_all(exe_dir);
        let _ = fs::remove_dir_all(fallback);
    }

    #[test]
    fn empty_pointer_file_falls_back_to_default_app_data_dir() {
        let exe_dir = temp_dir("pointer-empty-exe");
        let fallback = temp_dir("pointer-empty-fallback");
        fs::write(exe_dir.join(PROFILE_POINTER_FILENAME), "\n\n").expect("pointer file writable");

        let resolved = resolve_profile_data_dir(&exe_dir, fallback.clone(), None).unwrap();

        assert_eq!(resolved, fallback);
        let _ = fs::remove_dir_all(exe_dir);
        let _ = fs::remove_dir_all(fallback);
    }

    #[test]
    fn portable_profile_dir_next_to_exe_overrides_default_app_data_dir() {
        let exe_dir = temp_dir("portable-exe");
        let fallback = temp_dir("portable-fallback");
        let portable = exe_dir.join(PORTABLE_PROFILE_DIRNAME);
        fs::create_dir_all(&portable).expect("portable profile directory should be writable");

        let resolved = resolve_profile_data_dir(&exe_dir, fallback.clone(), None).unwrap();

        assert_eq!(resolved, portable);
        let _ = fs::remove_dir_all(exe_dir);
        let _ = fs::remove_dir_all(fallback);
    }

    #[test]
    fn falls_back_to_default_app_data_dir_when_no_override_exists() {
        let exe_dir = temp_dir("fallback-exe");
        let fallback = temp_dir("fallback-app-data");

        let resolved = resolve_profile_data_dir(&exe_dir, fallback.clone(), None).unwrap();

        assert_eq!(resolved, fallback);
        let _ = fs::remove_dir_all(exe_dir);
        let _ = fs::remove_dir_all(resolved);
    }

    #[test]
    fn storage_namespace_is_stable_and_does_not_expose_profile_path() {
        let first = profile_storage_namespace("profile-id-1");
        let second = profile_storage_namespace("profile-id-1");
        let different = profile_storage_namespace("profile-id-2");

        assert_eq!(first, second);
        assert_ne!(first, different);
        assert!(!first.contains("profile"));
    }

    #[test]
    fn profile_id_persists_across_profile_paths_instances() {
        let profile_dir = temp_dir("profile-id");

        let first = ProfilePaths::new(profile_dir.clone()).expect("profile paths");
        let second = ProfilePaths::new(profile_dir.clone()).expect("profile paths");
        assert_eq!(first.storage_namespace(), second.storage_namespace());
        assert!(profile_dir.join(PROFILE_ID_FILENAME).is_file());

        let _ = fs::remove_dir_all(profile_dir);
    }

    #[test]
    fn corrupt_profile_id_file_is_regenerated() {
        let profile_dir = temp_dir("profile-id-corrupt");
        fs::write(profile_dir.join(PROFILE_ID_FILENAME), "   \n").expect("id file writable");

        let paths = ProfilePaths::new(profile_dir.clone()).expect("profile paths");
        let regenerated = fs::read_to_string(profile_dir.join(PROFILE_ID_FILENAME))
            .expect("id file readable")
            .trim()
            .to_string();
        assert!(!regenerated.is_empty());
        assert_eq!(
            paths.storage_namespace(),
            profile_storage_namespace(&regenerated)
        );

        let _ = fs::remove_dir_all(profile_dir);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_profile_id_file_fails_instead_of_falling_back() {
        use std::os::unix::fs::PermissionsExt;

        let profile_dir = temp_dir("profile-id-unreadable");
        let id_path = profile_dir.join(PROFILE_ID_FILENAME);
        fs::write(&id_path, "existing-id").expect("id file writable");
        fs::set_permissions(&id_path, fs::Permissions::from_mode(0o000))
            .expect("permissions should be settable");

        let result = ProfilePaths::new(profile_dir.clone());

        assert!(result.is_err());

        let _ = fs::set_permissions(&id_path, fs::Permissions::from_mode(0o600));
        let _ = fs::remove_dir_all(profile_dir);
    }
}
