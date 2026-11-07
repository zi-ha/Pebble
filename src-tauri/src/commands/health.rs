use crate::state::AppState;
use semver::Version;
use serde::Serialize;
use tauri::State;

/// The repository the update check and the About page point at.
const REPO: &str = "zi-ha/Pebble";

#[derive(Serialize)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub release_url: String,
    pub is_newer: bool,
}

/// Parse a release tag into a comparable version.
///
/// Tags are written by hand, so accept `v1.2.3`, `1.2.3`, and the abbreviated
/// `1.2` / `0.20` that people publish. Missing components are padded with
/// zeros — otherwise `0.20` would fail to parse and fall back to a string
/// comparison, which reports nonsense for pairs like `0.20` vs `0.2`.
pub fn parse_release_version(raw: &str) -> Option<Version> {
    let trimmed = raw.trim();
    let trimmed = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let (core, suffix) = match trimmed.find(['-', '+']) {
        Some(index) => trimmed.split_at(index),
        None => (trimmed, ""),
    };

    let mut parts: Vec<&str> = core.split('.').take(3).collect();
    while parts.len() < 3 {
        parts.push("0");
    }
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }

    Version::parse(&format!("{}{suffix}", parts.join("."))).ok()
}

/// Decide whether `latest` is newer than `current`.
///
/// When either side cannot be parsed as a version, fall back to "the tags
/// differ" — better than staying silent on a release nobody can parse.
pub fn is_newer_than(latest: &str, current: &str) -> bool {
    match (
        parse_release_version(latest),
        parse_release_version(current),
    ) {
        (Some(latest), Some(current)) => latest > current,
        _ => latest.trim() != current.trim(),
    }
}

#[tauri::command]
pub async fn check_for_update(current_version: String) -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent("Pebble-Email-Client")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let resp = client
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("Failed to check for updates: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        // The unauthenticated endpoint answers 403 once the hourly rate limit is
        // used up, and 404 both for a missing repo and for "no release yet" on a
        // repo that is private.
        let hint = match status.as_u16() {
            404 => " (no published release, or the repository is not public)",
            403 | 429 => " (GitHub rate limit reached, try again later)",
            _ => "",
        };
        return Err(format!("GitHub API returned status {status}{hint}"));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    let tag = data["tag_name"]
        .as_str()
        .ok_or("Missing tag_name in response")?;
    let latest = tag
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string();
    let release_url = data["html_url"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://github.com/{REPO}/releases"));

    let has_update = is_newer_than(&latest, &current_version);

    Ok(UpdateInfo {
        is_newer: has_update,
        latest_version: latest,
        release_url,
    })
}

#[tauri::command]
pub fn open_default_mail_settings() -> Result<(), String> {
    #[cfg(windows)]
    {
        register_as_mail_client()?;
        opener::open("ms-settings:defaultapps").map_err(|e| format!("Failed to open settings: {e}"))
    }
    #[cfg(not(windows))]
    {
        Err("This feature is only available on Windows".to_string())
    }
}

#[cfg(windows)]
fn register_as_mail_client() -> Result<(), String> {
    use windows_registry::CURRENT_USER;

    let exe_path = tauri::utils::platform::current_exe()
        .map_err(|e| format!("Failed to get exe path: {e}"))?;
    let exe = exe_path.to_string_lossy();
    let open_cmd = format!("\"{exe}\" \"%1\"");

    // Register as a mail client
    let client_key = CURRENT_USER
        .create(r"SOFTWARE\Clients\Mail\Pebble")
        .map_err(|e| e.to_string())?;
    client_key
        .set_string("", "Pebble")
        .map_err(|e| e.to_string())?;

    let shell_key = CURRENT_USER
        .create(r"SOFTWARE\Clients\Mail\Pebble\shell\open\command")
        .map_err(|e| e.to_string())?;
    shell_key
        .set_string("", format!("\"{exe}\""))
        .map_err(|e| e.to_string())?;

    // Register capabilities
    let caps_key = CURRENT_USER
        .create(r"SOFTWARE\Clients\Mail\Pebble\Capabilities")
        .map_err(|e| e.to_string())?;
    caps_key
        .set_string("ApplicationName", "Pebble")
        .map_err(|e| e.to_string())?;
    caps_key
        .set_string("ApplicationDescription", "Pebble Email Client")
        .map_err(|e| e.to_string())?;

    let url_key = CURRENT_USER
        .create(r"SOFTWARE\Clients\Mail\Pebble\Capabilities\UrlAssociations")
        .map_err(|e| e.to_string())?;
    url_key
        .set_string("mailto", "Pebble.Url.mailto")
        .map_err(|e| e.to_string())?;

    // Register mailto protocol handler class
    let class_key = CURRENT_USER
        .create(r"SOFTWARE\Classes\Pebble.Url.mailto")
        .map_err(|e| e.to_string())?;
    class_key
        .set_string("", "Pebble Email URL")
        .map_err(|e| e.to_string())?;
    class_key
        .set_string("URL Protocol", "")
        .map_err(|e| e.to_string())?;

    let class_cmd_key = CURRENT_USER
        .create(r"SOFTWARE\Classes\Pebble.Url.mailto\shell\open\command")
        .map_err(|e| e.to_string())?;
    class_cmd_key
        .set_string("", &open_cmd)
        .map_err(|e| e.to_string())?;

    // Register in RegisteredApplications
    let reg_apps_key = CURRENT_USER
        .create(r"SOFTWARE\RegisteredApplications")
        .map_err(|e| e.to_string())?;
    reg_apps_key
        .set_string("Pebble", r"SOFTWARE\Clients\Mail\Pebble\Capabilities")
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn sync_titlebar_theme(window: tauri::WebviewWindow, theme: String) -> Result<(), String> {
    use tauri::window::Color;

    let color = match theme.as_str() {
        "dark" => Color(0x1e, 0x1e, 0x1e, 0xff),
        _ => Color(0xf8, 0xf7, 0xf5, 0xff),
    };
    window
        .set_background_color(Some(color))
        .map_err(|e| format!("Failed to set background color: {e}"))
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    // Only allow safe URL schemes to prevent command injection via opener::open / ShellExecuteW
    if !url.starts_with("https://") && !url.starts_with("http://") && !url.starts_with("mailto:") {
        return Err("Only https://, http://, and mailto: URLs are permitted".to_string());
    }
    opener::open(&url).map_err(|e| format!("Failed to open URL: {e}"))
}

#[tauri::command]
pub fn health_check(state: State<'_, AppState>) -> Result<String, String> {
    match state.store.list_accounts() {
        Ok(accounts) => Ok(format!(
            "Pebble is healthy. {} account(s) configured.",
            accounts.len()
        )),
        Err(e) => Err(format!("Health check failed: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_abbreviated_release_tags() {
        assert_eq!(
            parse_release_version("0.20").map(|v| v.to_string()),
            Some("0.20.0".to_string())
        );
        assert_eq!(
            parse_release_version("v0.2").map(|v| v.to_string()),
            Some("0.2.0".to_string())
        );
        assert_eq!(
            parse_release_version("V1.2.3").map(|v| v.to_string()),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn keeps_prerelease_and_build_metadata() {
        assert_eq!(
            parse_release_version("v0.2.0-beta.1").map(|v| v.to_string()),
            Some("0.2.0-beta.1".to_string())
        );
        assert_eq!(
            parse_release_version("1.2.3+build.5").map(|v| v.to_string()),
            Some("1.2.3+build.5".to_string())
        );
    }

    #[test]
    fn drops_extra_components_and_rejects_nonsense() {
        assert_eq!(
            parse_release_version("1.2.3.4").map(|v| v.to_string()),
            Some("1.2.3".to_string())
        );
        assert!(parse_release_version("nightly").is_none());
        assert!(parse_release_version("").is_none());
        assert!(parse_release_version("1..3").is_none());
    }

    #[test]
    fn compares_abbreviated_tags_numerically() {
        // The point of padding: `0.20` is newer than `0.1.6`, while plain string
        // comparison would order "0.20" before "0.1.6".
        assert!(is_newer_than("0.20", "0.1.6"));
        assert!(is_newer_than("v0.2.0", "0.1.6"));
        assert!(!is_newer_than("0.1.6", "0.1.6"));
        assert!(!is_newer_than("v0.1.5", "0.1.6"));
        assert!(is_newer_than("0.20", "0.2.0"));
    }

    #[test]
    fn falls_back_to_tag_inequality_for_unparseable_tags() {
        assert!(is_newer_than("nightly", "0.1.6"));
        assert!(!is_newer_than("nightly", "nightly"));
    }
}
