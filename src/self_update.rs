use anyhow::{bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

const REPOSITORY_URL: &str = "https://github.com/HoshiyomiLusia/envprobe";
const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/HoshiyomiLusia/envprobe/releases/latest";

#[derive(Debug, PartialEq, Eq)]
enum UpdateState {
    UpToDate,
    UpdateAvailable,
    CurrentNewer,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

#[derive(Debug, Clone, Copy)]
struct ReleaseTarget {
    /// `{OS}-{ARCH}`, matching release asset naming, e.g. `macos-aarch64`.
    label: &'static str,
    /// Archive extension without the leading dot: `tar.gz` or `zip`.
    ext: &'static str,
    /// Binary file name inside the archive: `envprobe` or `envprobe.exe`.
    binary: &'static str,
}

pub fn check_update() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let release = fetch_latest_release()?;
    let latest = release_version(&release.tag_name)?;
    let state = compare_versions(current, latest)?;

    match state {
        UpdateState::UpToDate => {
            println!("envprobe is up to date ({current}).");
        }
        UpdateState::UpdateAvailable => {
            println!("envprobe {latest} is available (current {current}).");
            println!("Run: envprobe update");
        }
        UpdateState::CurrentNewer => {
            println!("envprobe {current} is newer than the remote version ({latest}).");
        }
    }

    Ok(())
}

pub fn update() -> Result<()> {
    // Can't replace a running .exe on Windows; use the installer instead.
    if cfg!(target_os = "windows") {
        println!("envprobe update is not supported on Windows.");
        println!("Re-run the installer to update:");
        println!(
            "  irm https://raw.githubusercontent.com/HoshiyomiLusia/envprobe/main/install.ps1 | iex"
        );
        return Ok(());
    }

    let current = env!("CARGO_PKG_VERSION");
    let release = fetch_latest_release()?;
    let latest = release_version(&release.tag_name)?;
    let state = compare_versions(current, latest)?;

    match state {
        UpdateState::UpToDate => {
            println!("envprobe is already up to date ({current}).");
            return Ok(());
        }
        UpdateState::CurrentNewer => {
            println!("envprobe {current} is newer than the latest release ({latest}).");
            return Ok(());
        }
        UpdateState::UpdateAvailable => {}
    }

    let target = release_target()?;
    let tag = &release.tag_name;
    let name = format!("envprobe-{}-{}", tag, target.label);
    let asset = format!("{name}.{}", target.ext);
    let url = format!("{REPOSITORY_URL}/releases/download/{tag}/{asset}");
    let checksum_url = format!("{url}.sha256");
    let tmp_dir = create_temp_dir()?;
    let archive = tmp_dir.path().join(&asset);

    download_to_file(&url, &archive).context("failed to download release asset")?;
    let checksum = fetch_url(&checksum_url).context("failed to fetch release checksum")?;
    verify_checksum(&archive, &checksum).context("release checksum verification failed")?;
    extract_archive(&archive, tmp_dir.path()).context("failed to extract release asset")?;

    let extracted_binary = tmp_dir.path().join(&name).join(target.binary);
    if !extracted_binary.is_file() {
        bail!("release archive missing {}/{}", name, target.binary);
    }

    let installed = replace_current_binary(&extracted_binary)?;

    println!("Updated envprobe to {latest} at {}.", installed.display());

    Ok(())
}

fn fetch_latest_release() -> Result<GitHubRelease> {
    let text = fetch_url(LATEST_RELEASE_URL).context("failed to fetch latest GitHub release")?;
    serde_json::from_str(&text).context("failed to parse latest GitHub release")
}

fn release_version(tag: &str) -> Result<&str> {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(version).context("failed to parse latest release version")?;
    Ok(version)
}

fn release_target() -> Result<ReleaseTarget> {
    match (env::consts::OS, env::consts::ARCH) {
        ("windows", "x86_64") => Ok(ReleaseTarget {
            label: "windows-x86_64",
            ext: "zip",
            binary: "envprobe.exe",
        }),
        ("macos", "aarch64") => Ok(ReleaseTarget {
            label: "macos-aarch64",
            ext: "tar.gz",
            binary: "envprobe",
        }),
        ("linux", "x86_64") => Ok(ReleaseTarget {
            label: "linux-x86_64",
            ext: "tar.gz",
            binary: "envprobe",
        }),
        ("linux", "aarch64") => Ok(ReleaseTarget {
            label: "linux-aarch64",
            ext: "tar.gz",
            binary: "envprobe",
        }),
        (os, arch) => bail!(
            "no prebuilt envprobe release for {os} {arch}; build from source with: cargo install --git {REPOSITORY_URL}"
        ),
    }
}

fn download_to_file(url: &str, path: &Path) -> Result<()> {
    let curl = which::which("curl")
        .context("curl is required to download envprobe releases; install curl and try again")?;
    let status = Command::new(curl)
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            concat!("User-Agent: envprobe/", env!("CARGO_PKG_VERSION")),
            "--output",
        ])
        .arg(path)
        .arg(url)
        .status()
        .context("failed to start curl")?;

    if !status.success() {
        bail!("failed to download {url}: {status}");
    }

    Ok(())
}

fn extract_archive(archive: &Path, destination: &Path) -> Result<()> {
    let tar =
        which::which("tar").context("tar is required to extract envprobe release archives")?;
    let is_zip = archive
        .extension()
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false);

    let mut cmd = Command::new(tar);
    if is_zip {
        // bsdtar (preinstalled on Windows 10+ and macOS) auto-detects zip via -xf.
        cmd.arg("-xf");
    } else {
        cmd.arg("-xzf");
    }
    cmd.arg(archive).arg("-C").arg(destination);

    let status = cmd.status().context("failed to start tar")?;
    if !status.success() {
        bail!("tar failed with status {status}");
    }

    Ok(())
}

fn fetch_url(url: &str) -> Result<String> {
    let curl = which::which("curl")
        .context("curl is required to check for updates; install curl and try again")?;
    let output = Command::new(curl)
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            concat!("User-Agent: envprobe/", env!("CARGO_PKG_VERSION")),
            url,
        ])
        .output()
        .context("failed to start curl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("failed to fetch {url}: {stderr}");
    }

    String::from_utf8(output.stdout).context("remote response was not UTF-8")
}

fn verify_checksum(path: &Path, checksum_text: &str) -> Result<()> {
    let expected = parse_expected_checksum(checksum_text)?;
    let actual = sha256_file(path)?;

    if actual != expected {
        bail!("checksum mismatch for {}", path.display());
    }

    Ok(())
}

fn parse_expected_checksum(text: &str) -> Result<String> {
    let checksum = text
        .split_whitespace()
        .next()
        .context("checksum file was empty")?
        .to_ascii_lowercase();

    if checksum.len() != 64 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("checksum file did not start with a SHA-256 digest");
    }

    Ok(checksum)
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn create_temp_dir() -> Result<TempDir> {
    let path = env::temp_dir().join(format!("envprobe-update-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).context("failed to clean old update directory")?;
    }
    fs::create_dir_all(&path).context("failed to create update directory")?;
    Ok(TempDir { path })
}

fn replace_current_binary(new_binary: &Path) -> Result<PathBuf> {
    let current = env::current_exe().context("failed to locate current envprobe binary")?;
    let directory = current
        .parent()
        .context("current executable has no parent directory")?;
    let staged = directory.join(format!(".envprobe-update-{}", std::process::id()));

    if let Err(error) = fs::copy(new_binary, &staged) {
        if error.kind() == ErrorKind::PermissionDenied {
            bail!(
                "cannot write to {}; reinstall the latest release with: curl -fsSL https://raw.githubusercontent.com/HoshiyomiLusia/envprobe/main/install.sh | sh",
                directory.display()
            );
        }

        return Err(error).with_context(|| {
            format!(
                "failed to stage update in {}; check write permission",
                directory.display()
            )
        });
    }

    set_executable(&staged)?;
    if let Err(error) = fs::rename(&staged, &current) {
        let _ = fs::remove_file(&staged);
        if error.kind() == ErrorKind::PermissionDenied {
            bail!(
                "cannot replace {}; reinstall the latest release with: curl -fsSL https://raw.githubusercontent.com/HoshiyomiLusia/envprobe/main/install.sh | sh",
                current.display()
            );
        }

        return Err(error).with_context(|| {
            format!(
                "failed to replace {}; check write permission",
                current.display()
            )
        });
    }

    Ok(current)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .context("failed to set executable permission")
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn compare_versions(current: &str, latest: &str) -> Result<UpdateState> {
    let current = Version::parse(current).context("failed to parse current version")?;
    let latest = Version::parse(latest).context("failed to parse remote version")?;

    if latest > current {
        Ok(UpdateState::UpdateAvailable)
    } else if latest < current {
        Ok(UpdateState::CurrentNewer)
    } else {
        Ok(UpdateState::UpToDate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_version_from_tag() {
        let version = release_version("v1.2.3").unwrap();

        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn detects_available_update() {
        let state = compare_versions("1.2.3", "1.2.4").unwrap();

        assert_eq!(state, UpdateState::UpdateAvailable);
    }

    #[test]
    fn detects_up_to_date_version() {
        let state = compare_versions("1.2.3", "1.2.3").unwrap();

        assert_eq!(state, UpdateState::UpToDate);
    }

    #[test]
    fn parses_github_release_tag() {
        let release: GitHubRelease =
            serde_json::from_str(r#"{"tag_name":"v1.2.3","name":"envprobe 1.2.3"}"#).unwrap();

        assert_eq!(release.tag_name, "v1.2.3");
    }

    #[test]
    fn parses_release_checksum_file() {
        let checksum = parse_expected_checksum(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  envprobe.tar.gz",
        )
        .unwrap();

        assert_eq!(
            checksum,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verifies_release_checksum() {
        let path = env::temp_dir().join(format!("envprobe-checksum-test-{}", std::process::id()));
        fs::write(&path, b"abc").unwrap();

        let result = verify_checksum(
            &path,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  envprobe.tar.gz",
        );

        let _ = fs::remove_file(path);
        result.unwrap();
    }

    #[test]
    fn temp_dir_is_removed_on_drop() {
        let temp_dir = create_temp_dir().unwrap();
        let path = temp_dir.path().to_path_buf();

        assert!(path.is_dir());
        drop(temp_dir);
        assert!(!path.exists());
    }

    #[test]
    fn release_target_uses_os_arch_naming() {
        let target = release_target().unwrap();
        assert!(target.label.contains('-'));
        assert!(matches!(target.ext, "tar.gz" | "zip"));
        assert!(matches!(target.binary, "envprobe" | "envprobe.exe"));
    }
}
