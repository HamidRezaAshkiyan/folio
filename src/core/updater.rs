use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const REPO: &str = "HamidRezaAshkiyan/folio";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub asset_name: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallOutcome {
    pub updated_to: String,
    pub target_path: String,
    pub message: String,
    pub needs_restart: bool,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Remove any leftover `.old` executable from a previous Windows self-update.
pub fn cleanup_old_binary() {
    if let Ok(exe) = std::env::current_exe() {
        let old = old_backup_path(&exe);
        if old.exists() {
            let _ = fs::remove_file(old);
        }
    }
}

fn old_backup_path(exe: &Path) -> PathBuf {
    let mut os = exe.as_os_str().to_os_string();
    os.push(".old");
    PathBuf::from(os)
}

/// Parse a semver-like string (with optional leading `v` and optional `-prerelease`) into `(major, minor, patch)`.
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64)> {
    let trimmed = raw.trim().trim_start_matches('v').trim_start_matches('V');
    let core = trimmed.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

/// Return `true` when `latest` is strictly greater than `current`.
pub fn is_newer_version(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(cur), Some(lat)) => lat > cur,
        _ => false,
    }
}

/// Return the portable archive asset name for the current OS & CPU architecture.
pub fn platform_asset_name() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "folio-macos-arm64.tar.gz"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "folio-macos-x64.tar.gz"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "folio-windows-x64.zip"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "folio-linux-x64.tar.gz"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        "folio-linux-x64.tar.gz"
    }
}

/// Query GitHub Releases for the latest Folio version.
pub fn check_for_updates() -> Result<UpdateInfo> {
    let api_url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let raw_json = fetch_text(&api_url)?;
    let release: GithubRelease = serde_json::from_str(&raw_json)
        .context("failed to parse GitHub latest release response")?;

    let latest_clean = release
        .tag_name
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string();
    let asset_name = platform_asset_name().to_string();
    let download_url = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .map(|a| a.browser_download_url.clone())
        .unwrap_or_else(|| {
            format!(
                "https://github.com/{REPO}/releases/download/{}/{}",
                release.tag_name, asset_name
            )
        });

    let has_update = is_newer_version(CURRENT_VERSION, &latest_clean);

    Ok(UpdateInfo {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_clean,
        has_update,
        release_url: release.html_url,
        asset_name,
        download_url,
    })
}

/// Download and install the release described by `info`.
///
/// Replaces the currently running binary in place when writable; otherwise falls back to
/// launching the native OS installer (`.pkg` on macOS, `setup.exe` on Windows) or installing
/// into `~/.local/bin/folio` on Linux.
pub fn install_update(info: &UpdateInfo) -> Result<InstallOutcome> {
    let tmp_dir = std::env::temp_dir().join(format!(
        "folio-update-{}-{}",
        std::process::id(),
        chrono::Local::now().timestamp()
    ));
    fs::create_dir_all(&tmp_dir)
        .with_context(|| format!("failed to create temp dir {}", tmp_dir.display()))?;

    let result = install_update_inner(info, &tmp_dir);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

fn install_update_inner(info: &UpdateInfo, tmp_dir: &Path) -> Result<InstallOutcome> {
    let archive_path = tmp_dir.join(&info.asset_name);
    download_file(&info.download_url, &archive_path)?;

    let extracted_bin = extract_binary(&archive_path, tmp_dir)?;
    let current_exe = std::env::current_exe().context("could not locate running folio binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&extracted_bin, fs::Permissions::from_mode(0o755));

        match atomic_replace_unix(&extracted_bin, &current_exe) {
            Ok(()) => Ok(InstallOutcome {
                updated_to: info.latest_version.clone(),
                target_path: current_exe.to_string_lossy().to_string(),
                message: format!(
                    "Updated Folio to v{} at {}. Restart Folio to use the new version.",
                    info.latest_version,
                    current_exe.display()
                ),
                needs_restart: true,
            }),
            Err(_perm_err) => fallback_unix_install(info, &extracted_bin),
        }
    }

    #[cfg(windows)]
    {
        match atomic_replace_windows(&extracted_bin, &current_exe) {
            Ok(()) => Ok(InstallOutcome {
                updated_to: info.latest_version.clone(),
                target_path: current_exe.to_string_lossy().to_string(),
                message: format!(
                    "Updated Folio to v{} at {}. Restart Folio to use the new version.",
                    info.latest_version,
                    current_exe.display()
                ),
                needs_restart: true,
            }),
            Err(_perm_err) => fallback_windows_installer(info),
        }
    }
}

#[cfg(unix)]
fn atomic_replace_unix(new_bin: &Path, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("target binary has no parent directory"))?;
    let staged = parent.join(format!(".folio-update-{}.tmp", std::process::id()));
    fs::copy(new_bin, &staged)
        .with_context(|| format!("failed to stage binary in {}", parent.display()))?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
    if let Err(e) = fs::rename(&staged, target) {
        let _ = fs::remove_file(&staged);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(unix)]
fn fallback_unix_install(info: &UpdateInfo, extracted_bin: &Path) -> Result<InstallOutcome> {
    #[cfg(target_os = "macos")]
    {
        // Download the Universal macOS .pkg installer and launch macOS Installer.app
        let pkg_url = format!(
            "https://github.com/{REPO}/releases/download/v{}/folio-macos-universal.pkg",
            info.latest_version
        );
        let pkg_path = std::env::temp_dir().join(format!("folio-{}.pkg", info.latest_version));
        if download_file(&pkg_url, &pkg_path).is_ok() && open::that(&pkg_path).is_ok() {
            return Ok(InstallOutcome {
                updated_to: info.latest_version.clone(),
                target_path: pkg_path.to_string_lossy().to_string(),
                message: format!(
                    "Downloaded Folio v{} installer (.pkg) and opened macOS Installer.",
                    info.latest_version
                ),
                needs_restart: true,
            });
        }
    }

    // Fallback to user-writable ~/.local/bin/folio
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not resolve home directory"))?;
    let local_bin = home.join(".local").join("bin");
    fs::create_dir_all(&local_bin)?;
    let dest = local_bin.join("folio");
    atomic_replace_unix(extracted_bin, &dest)?;
    Ok(InstallOutcome {
        updated_to: info.latest_version.clone(),
        target_path: dest.to_string_lossy().to_string(),
        message: format!(
            "Installed Folio v{} to {} (original location required elevated permissions).",
            info.latest_version,
            dest.display()
        ),
        needs_restart: true,
    })
}

#[cfg(windows)]
fn atomic_replace_windows(new_bin: &Path, target: &Path) -> Result<()> {
    let backup = old_backup_path(target);
    if backup.exists() {
        let _ = fs::remove_file(&backup);
    }
    fs::rename(target, &backup)?;
    if let Err(e) = fs::copy(new_bin, target) {
        let _ = fs::rename(&backup, target);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(windows)]
fn fallback_windows_installer(info: &UpdateInfo) -> Result<InstallOutcome> {
    let exe_url = format!(
        "https://github.com/{REPO}/releases/download/v{}/folio-windows-x64-setup.exe",
        info.latest_version
    );
    let exe_path = std::env::temp_dir().join(format!("folio-{}-setup.exe", info.latest_version));
    download_file(&exe_url, &exe_path)?;
    open::that(&exe_path)?;
    Ok(InstallOutcome {
        updated_to: info.latest_version.clone(),
        target_path: exe_path.to_string_lossy().to_string(),
        message: format!(
            "Downloaded Folio v{} installer and launched setup wizard.",
            info.latest_version
        ),
        needs_restart: true,
    })
}

fn fetch_text(url: &str) -> Result<String> {
    #[cfg(not(target_os = "windows"))]
    {
        let out = Command::new("curl")
            .args([
                "-fsSL",
                "--max-time",
                "10",
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                &format!("User-Agent: folio/{CURRENT_VERSION}"),
                url,
            ])
            .output()
            .context("failed to run curl for update check")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("update check failed: {}", stderr.trim()));
        }
        String::from_utf8(out.stdout).context("invalid UTF-8 in update response")
    }

    #[cfg(target_os = "windows")]
    {
        let ps = format!(
            "$ProgressPreference = 'SilentlyContinue'; (Invoke-WebRequest -UseBasicParsing -TimeoutSec 10 -Headers @{{'User-Agent'='folio/{CURRENT_VERSION}';'Accept'='application/vnd.github+json'}} -Uri '{url}').Content"
        );
        let out = Command::new("pwsh")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .or_else(|_| {
                Command::new("powershell")
                    .args(["-NoProfile", "-Command", &ps])
                    .output()
            })
            .context("failed to run PowerShell for update check")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("update check failed: {}", stderr.trim()));
        }
        String::from_utf8(out.stdout).context("invalid UTF-8 in update response")
    }
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        let out = Command::new("curl")
            .args([
                "-fsSL",
                "--max-time",
                "120",
                "-H",
                &format!("User-Agent: folio/{CURRENT_VERSION}"),
                url,
                "-o",
            ])
            .arg(dest)
            .output()
            .context("failed to run curl to download update")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("download failed: {}", stderr.trim()));
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let dest_str = dest.to_string_lossy().replace('\'', "''");
        let ps = format!(
            "$ProgressPreference = 'SilentlyContinue'; Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Headers @{{'User-Agent'='folio/{CURRENT_VERSION}'}} -Uri '{url}' -OutFile '{dest_str}'"
        );
        let out = Command::new("pwsh")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .or_else(|_| {
                Command::new("powershell")
                    .args(["-NoProfile", "-Command", &ps])
                    .output()
            })
            .context("failed to run PowerShell to download update")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("download failed: {}", stderr.trim()));
        }
        Ok(())
    }
}

fn extract_binary(archive_path: &Path, dest_dir: &Path) -> Result<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    {
        let out = Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(dest_dir)
            .output()
            .context("failed to extract update archive with tar")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("archive extraction failed: {}", stderr.trim()));
        }
        let bin = dest_dir.join("folio");
        if !bin.exists() {
            return Err(anyhow!("extracted archive did not contain 'folio' binary"));
        }
        Ok(bin)
    }

    #[cfg(target_os = "windows")]
    {
        let arch = archive_path.to_string_lossy().replace('\'', "''");
        let dir = dest_dir.to_string_lossy().replace('\'', "''");
        let ps = format!("Expand-Archive -Path '{arch}' -DestinationPath '{dir}' -Force");
        let out = Command::new("pwsh")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .or_else(|_| {
                Command::new("powershell")
                    .args(["-NoProfile", "-Command", &ps])
                    .output()
            })
            .context("failed to extract update zip")?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("zip extraction failed: {}", stderr.trim()));
        }
        let bin = dest_dir.join("folio.exe");
        if !bin.exists() {
            return Err(anyhow!("extracted archive did not contain 'folio.exe'"));
        }
        Ok(bin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_versions_correctly() {
        assert!(is_newer_version("1.9.0", "1.10.0"));
        assert!(is_newer_version("1.10.0", "v1.10.1"));
        assert!(is_newer_version("1.10.0", "v2.0.0"));
        assert!(!is_newer_version("1.10.0", "v1.10.0"));
        assert!(!is_newer_version("1.10.0", "1.9.9"));
        assert!(!is_newer_version("1.10.0", "invalid"));
    }

    #[test]
    fn resolves_platform_asset_name() {
        let name = platform_asset_name();
        assert!(name.starts_with("folio-"));
        assert!(name.ends_with(".tar.gz") || name.ends_with(".zip"));
    }
}
