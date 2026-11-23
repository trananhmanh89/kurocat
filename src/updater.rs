//! Update checks against GitHub Releases and self-install of new versions.
//!
//! Installed copies (with `uninstall.exe` next to the exe) update through the new setup.exe;
//! portable copies replace their own exe. Downloads go through PowerShell and are SHA-256 checked
//! when the release provides a digest.

use crate::events::{self, AppEvent};
use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const REPO_API_LATEST: &str = "https://api.github.com/repos/qianyiai/kurocat/releases/latest";
const REPO_RELEASES_PAGE: &str = "https://github.com/qianyiai/kurocat/releases/latest";
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// The running version.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result of an update check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// The running version.
    pub current_version: String,
    /// The newest released version.
    pub latest_version: String,
    /// True when the newest version is newer than the running one.
    pub has_update: bool,
    /// Web page of the release.
    pub release_url: String,
    /// File name of the matching download, if any.
    pub asset_name: Option<String>,
    /// Download URL of the matching file, if any.
    pub download_url: Option<String>,
    /// Expected SHA-256 of the download (lowercase hex), if GitHub provides it.
    pub asset_sha256: Option<String>,
    /// Size of the download in bytes, if known.
    pub asset_size: Option<u64>,
    /// "windows-nsis" for installed copies, "windows-portable" otherwise.
    pub install_kind: String,
    /// The first lines of the release notes.
    pub notes_summary: String,
}

/// Progress of an update check or install, shown in the toast.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressEvent {
    /// One of: checking, available, up_to_date, downloading, verifying, installing, error.
    pub stage: String,
    /// Text for the user.
    pub message: String,
    /// The update being processed, if known.
    pub info: Option<UpdateInfo>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: Option<String>,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
    digest: Option<String>,
}

/// Parses "v1.2.3" or "1.2.3-beta" into numbers. Missing parts count as 0.
///
/// @param raw The version text.
/// @return (major, minor, patch).
pub fn parse_semver(raw: &str) -> (u64, u64, u64) {
    let clean = raw.trim().trim_start_matches(['v', 'V']);
    let main = clean.split('-').next().unwrap_or(clean);
    let mut parts = main.split('.');
    let major = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    (major, minor, patch)
}

/// Compares two versions.
///
/// @param latest The candidate version.
/// @param current The running version.
/// @return True if `latest` is newer than `current`.
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    parse_semver(latest) > parse_semver(current)
}

/// Extracts a SHA-256 hex digest from GitHub's "sha256:<hex>" asset digest.
///
/// @param digest The digest field from the GitHub API.
/// @return The lowercase hex digest, or None if missing or malformed.
fn extract_sha256(digest: &Option<String>) -> Option<String> {
    let raw = digest.as_deref()?.trim();
    let hex = raw.strip_prefix("sha256:").unwrap_or(raw).trim().to_ascii_lowercase();
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex)
    } else {
        None
    }
}

/// Works out how this copy was installed.
///
/// @return "windows-nsis" if an uninstaller sits next to the exe, otherwise "windows-portable".
fn detect_install_kind() -> &'static str {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("uninstall.exe").exists() {
                return "windows-nsis";
            }
        }
    }
    "windows-portable"
}

/// Checks whether a release file name is a setup installer for x64.
///
/// @param lower_name The file name in lowercase.
/// @return True for `*-setup.exe` files built for x64.
fn is_setup_asset(lower_name: &str) -> bool {
    lower_name.ends_with("-setup.exe") && lower_name.contains("x64")
}

/// Checks whether a release file name is the portable exe.
///
/// @param lower_name The file name in lowercase.
/// @return True for `*-windows-x64.exe` files.
fn is_portable_asset(lower_name: &str) -> bool {
    lower_name.ends_with("-windows-x64.exe")
}

/// Picks the release file that matches this copy (installer or portable, 65% or 108-key).
///
/// @param assets The files attached to the release.
/// @param install_kind "windows-nsis" or "windows-portable".
/// @return The best matching file, or None.
fn select_asset<'a>(assets: &'a [GhAsset], install_kind: &str) -> Option<&'a GhAsset> {
    let want_108 = cfg!(feature = "layout-108");
    let lower_names: Vec<String> = assets.iter().map(|a| a.name.to_ascii_lowercase()).collect();

    let find = |test: &dyn Fn(&str) -> bool| -> Option<&'a GhAsset> {
        for (index, name) in lower_names.iter().enumerate() {
            if test(name) {
                return Some(&assets[index]);
            }
        }
        None
    };

    if install_kind == "windows-nsis" {
        find(&|n| is_setup_asset(n) && n.contains("108key") == want_108)
            .or_else(|| find(&|n| is_setup_asset(n)))
            .or_else(|| find(&|n| is_portable_asset(n)))
    } else {
        find(&|n| is_portable_asset(n) && n.contains("108key") == want_108)
            .or_else(|| find(&|n| is_portable_asset(n)))
            .or_else(|| find(&|n| n.ends_with("-setup.exe")))
    }
}

/// Shortens release notes to their first three non-heading lines (at most 180 characters).
///
/// @param body The release notes in Markdown.
/// @return The short summary.
fn summarize_notes(body: Option<&str>) -> String {
    let Some(raw) = body else {
        return String::new();
    };
    let lines: Vec<&str> = raw
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .take(3)
        .collect();
    lines.join(" ").chars().take(180).collect()
}

/// Runs a hidden PowerShell script and waits for it.
///
/// @param script The PowerShell command text.
/// @return The process output, or an error if PowerShell could not start.
fn run_hidden_powershell(script: &str) -> Result<std::process::Output, String> {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("Could not start PowerShell: {}", error))
}

/// Downloads the latest release description from the GitHub API.
///
/// @return The raw JSON text, or an error message.
fn fetch_latest_release_json() -> Result<String, String> {
    let script = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         $ProgressPreference = 'SilentlyContinue'; \
         $resp = Invoke-WebRequest -UseBasicParsing -Uri '{url}' \
           -Headers @{{'User-Agent'='KuroCat/{ver}'; 'Accept'='application/vnd.github+json'}} -TimeoutSec 15; \
         Write-Output $resp.Content",
        url = REPO_API_LATEST,
        ver = CURRENT_VERSION
    );

    let output = run_hidden_powershell(&script)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err("Could not reach GitHub to check for updates. Check your network or proxy.".to_string());
        }
        let first_line = stderr.lines().next().unwrap_or(&stderr);
        return Err(format!("Update check failed: {}", first_line));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("GitHub returned an empty response".to_string());
    }
    Ok(text)
}

/// Asks GitHub for the latest release and compares it with the running version.
///
/// @return The update information, or an error message.
pub fn check_latest_update() -> Result<UpdateInfo, String> {
    let raw_json = fetch_latest_release_json()?;
    let release: GhRelease = serde_json::from_str(&raw_json)
        .map_err(|error| format!("Could not read the release information: {}", error))?;

    let latest_clean = release.tag_name.trim().trim_start_matches(['v', 'V']).to_string();
    let has_update = is_newer_version(&latest_clean, CURRENT_VERSION);
    let install_kind = detect_install_kind();
    let asset = select_asset(&release.assets, install_kind);

    Ok(UpdateInfo {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_clean,
        has_update,
        release_url: release.html_url.unwrap_or_else(|| REPO_RELEASES_PAGE.to_string()),
        asset_name: asset.map(|a| a.name.clone()),
        download_url: asset.map(|a| a.browser_download_url.clone()),
        asset_sha256: asset.and_then(|a| extract_sha256(&a.digest)),
        asset_size: asset.and_then(|a| a.size),
        install_kind: install_kind.to_string(),
        notes_summary: summarize_notes(release.body.as_deref()),
    })
}

/// Sends update progress to the UI thread (shown in the toast).
///
/// @param stage The stage name (see [`UpdateProgressEvent::stage`]).
/// @param message Text for the user.
/// @param info The update being processed, if known.
pub fn emit_progress(stage: &str, message: impl Into<String>, info: Option<UpdateInfo>) {
    events::send(AppEvent::UpdateStatus(UpdateProgressEvent {
        stage: stage.to_string(),
        message: message.into(),
        info,
    }));
}

/// Opens a URL in the default browser.
///
/// @param url The address to open.
pub fn open_url_in_browser(url: &str) {
    let _ = Command::new("cmd.exe")
        .args(["/C", "start", "", url])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

/// Downloads a file and checks its SHA-256 if one is given.
///
/// @param url The download URL.
/// @param dest Where to save the file.
/// @param expected_sha256 The expected lowercase hex digest, if known.
/// @return Ok, or an error message.
fn download_and_verify(url: &str, dest: &PathBuf, expected_sha256: Option<&str>) -> Result<(), String> {
    let url_escaped = url.replace('\'', "''");
    let dest_escaped = dest.to_string_lossy().replace('\'', "''");
    let sha_escaped = expected_sha256.unwrap_or("").replace('\'', "''");

    let script = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         $ProgressPreference = 'SilentlyContinue'; \
         $dst = '{dst}'; \
         Invoke-WebRequest -UseBasicParsing -Uri '{url}' -Headers @{{'User-Agent'='KuroCat/{ver}'}} -OutFile $dst -TimeoutSec 180; \
         if (-not (Test-Path $dst) -or (Get-Item $dst).Length -lt 200000) {{ throw 'The download is incomplete' }}; \
         $exp = '{sha}'; \
         if ($exp -ne '') {{ \
           $actual = (Get-FileHash -Path $dst -Algorithm SHA256).Hash.ToLowerInvariant(); \
           if ($actual -ne $exp.ToLowerInvariant()) {{ Remove-Item $dst -Force -ErrorAction SilentlyContinue; throw \"SHA-256 check failed: $actual\" }} \
         }}",
        dst = dest_escaped,
        url = url_escaped,
        ver = CURRENT_VERSION,
        sha = sha_escaped,
    );

    let output = run_hidden_powershell(&script)?;
    if !output.status.success() {
        let error_text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if error_text.is_empty() {
            return Err("Downloading the update failed".to_string());
        }
        return Err(error_text.lines().next().unwrap_or(&error_text).to_string());
    }
    Ok(())
}

/// Starts a hidden PowerShell script that waits for this process to exit, then runs `then`.
///
/// @param then PowerShell commands to run after this process is gone.
/// @return Ok, or an error message if PowerShell could not start.
fn spawn_after_exit_script(then: &str) -> Result<(), String> {
    let pid = std::process::id();
    let script = format!(
        "for ($i = 0; $i -lt 40; $i++) {{ \
           if (-not (Get-Process -Id {pid} -ErrorAction SilentlyContinue)) {{ break }} \
           Start-Sleep -Milliseconds 200 \
         }}; \
         Start-Sleep -Milliseconds 250; \
         {then}",
        pid = pid,
        then = then,
    );
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("Could not start the update script: {}", error))?;
    Ok(())
}

/// Downloads and installs an update, then quits so the new version can start.
///
/// @param info The update to install.
/// @return Ok once the installer or replace script is running, or an error message.
fn install_windows_update(info: &UpdateInfo) -> Result<(), String> {
    let url = match info.download_url.as_deref() {
        Some(url) => url,
        None => return Err("No matching Windows download was found in the release".to_string()),
    };
    let asset_name = info.asset_name.as_deref().unwrap_or("KuroCat-update.exe");

    emit_progress("downloading", format!("Downloading v{}...", info.latest_version), Some(info.clone()));

    let temp_dir = std::env::temp_dir().join("kurocat-updates");
    let _ = std::fs::create_dir_all(&temp_dir);
    let safe_name = asset_name.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_");
    let downloaded_file = temp_dir.join(safe_name);

    download_and_verify(url, &downloaded_file, info.asset_sha256.as_deref())?;

    emit_progress(
        "installing",
        format!("Verified. Installing v{} and restarting...", info.latest_version),
        Some(info.clone()),
    );

    let downloaded_escaped = downloaded_file.to_string_lossy().replace('\'', "''");
    let is_setup_installer = asset_name.to_ascii_lowercase().ends_with("-setup.exe");

    if is_setup_installer {
        // Run the installer silently once we have exited, then start the installed exe. This copy
        // is the installed one (uninstall.exe sits next to it), and the installer reinstalls into
        // the same folder, which the user may have chosen.
        let relaunch_path = std::env::current_exe().map_err(|error| error.to_string())?;
        let relaunch_escaped = relaunch_path.to_string_lossy().replace('\'', "''");
        let then = format!(
            "Start-Process -FilePath '{setup}' -ArgumentList '/S' -Wait; \
             Remove-Item -LiteralPath '{setup}' -Force -ErrorAction SilentlyContinue; \
             Start-Process -FilePath '{exe}'",
            setup = downloaded_escaped,
            exe = relaunch_escaped,
        );
        spawn_after_exit_script(&then)?;
    } else {
        // Portable copy: replace our own exe once we have exited, then start it again.
        let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let current_escaped = current_exe.to_string_lossy().replace('\'', "''");
        let then = format!(
            "Copy-Item -LiteralPath '{src}' -Destination '{dst}' -Force; \
             Remove-Item -LiteralPath '{src}' -Force -ErrorAction SilentlyContinue; \
             Start-Process -FilePath '{dst}'",
            src = downloaded_escaped,
            dst = current_escaped,
        );
        spawn_after_exit_script(&then)?;
    }

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(450));
        events::send(AppEvent::Quit);
    });
    Ok(())
}

/// Installs an update if there is one, reporting progress to the toast.
///
/// @param info The result of a previous [`check_latest_update`].
/// @return Ok, or an error message.
pub fn perform_update_install(info: UpdateInfo) -> Result<(), String> {
    if !info.has_update {
        emit_progress(
            "up_to_date",
            format!("You already have the latest version (v{})", info.current_version),
            Some(info),
        );
        return Ok(());
    }
    install_windows_update(&info)
}

/// Starts a thread that checks for updates 8 seconds after start-up and then every 6 hours.
pub fn start_background_update_checker() {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(8));
        loop {
            if let Ok(info) = check_latest_update() {
                if info.has_update {
                    let message = format!("New version available: v{}", info.latest_version);
                    emit_progress("available", message, Some(info));
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_works() {
        assert!(is_newer_version("4.4.0", "4.3.0"));
        assert!(is_newer_version("v4.10.0", "4.4.0"));
        assert!(is_newer_version("5.0.0", "4.9.9"));
        assert!(!is_newer_version("4.4.0", "4.4.0"));
        assert!(!is_newer_version("v4.3.9", "4.4.0"));
    }

    #[test]
    fn sha256_extraction_validates_hex() {
        let valid = Some("sha256:05c1be2c444f5303770f6d59e216bc0ed7b69321fc44368e4ec231405570f950".to_string());
        assert_eq!(
            extract_sha256(&valid).as_deref(),
            Some("05c1be2c444f5303770f6d59e216bc0ed7b69321fc44368e4ec231405570f950")
        );
        assert_eq!(extract_sha256(&Some("sha256:short".to_string())), None);
    }

    #[test]
    fn asset_selection_prefers_matching_kind() {
        let assets = vec![
            GhAsset {
                name: "KuroCat-5.0.0-windows-x64.exe".to_string(),
                browser_download_url: "p".to_string(),
                size: None,
                digest: None,
            },
            GhAsset {
                name: "KuroCat-5.0.0-x64-setup.exe".to_string(),
                browser_download_url: "s".to_string(),
                size: None,
                digest: None,
            },
        ];
        assert_eq!(select_asset(&assets, "windows-nsis").map(|a| a.browser_download_url.as_str()), Some("s"));
        assert_eq!(select_asset(&assets, "windows-portable").map(|a| a.browser_download_url.as_str()), Some("p"));
    }
}
