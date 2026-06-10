use crate::model::{Category, Finding, FindingKind, Status};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub async fn probe_all(timeout_ms: u64) -> Vec<Finding> {
    let (pyenv, nvm, rbenv, asdf, conda) = tokio::join!(
        probe_pyenv(timeout_ms),
        probe_nvm(),
        probe_rbenv(timeout_ms),
        probe_asdf(timeout_ms),
        probe_conda(timeout_ms),
    );
    let mut findings = Vec::new();
    findings.extend(pyenv);
    findings.extend(nvm);
    findings.extend(rbenv);
    findings.extend(asdf);
    findings.extend(conda);
    findings
}

async fn probe_pyenv(timeout_ms: u64) -> Vec<Finding> {
    let Ok(path) = which::which("pyenv") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let version = run_extract(&path, &["--version"], r"pyenv (\S+)", timeout_ms).await;
    out.push(Finding {
        name: "pyenv".to_string(),
        category: Category::VersionManager,
        kind: FindingKind::VersionManager,
        status: Status::Installed,
        version,
        path: Some(path.clone()),
        detail: None,
    });
    for v in run_lines(&path, &["versions", "--bare"], timeout_ms).await {
        let v = v.trim().to_string();
        if v.is_empty() || v == "system" {
            continue;
        }
        out.push(Finding {
            name: "python".to_string(),
            category: Category::Language,
            kind: FindingKind::ManagedRuntime {
                manager: "pyenv".to_string(),
            },
            status: Status::Installed,
            version: Some(v),
            path: None,
            detail: None,
        });
    }
    out
}

async fn probe_nvm() -> Vec<Finding> {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Vec::new();
    }
    let nvm_dir = PathBuf::from(&home).join(".nvm");
    if !nvm_dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if nvm_dir.join("nvm.sh").exists() {
        out.push(Finding {
            name: "nvm".to_string(),
            category: Category::VersionManager,
            kind: FindingKind::VersionManager,
            status: Status::Installed,
            version: None,
            path: Some(nvm_dir.clone()),
            detail: Some("shell function".to_string()),
        });
    }
    let node_dir = nvm_dir.join("versions").join("node");
    if let Ok(entries) = std::fs::read_dir(&node_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let version = name.trim_start_matches('v').to_string();
            if version.is_empty() {
                continue;
            }
            out.push(Finding {
                name: "node".to_string(),
                category: Category::Language,
                kind: FindingKind::ManagedRuntime {
                    manager: "nvm".to_string(),
                },
                status: Status::Installed,
                version: Some(version),
                path: Some(entry.path()),
                detail: None,
            });
        }
    }
    out
}

async fn probe_rbenv(timeout_ms: u64) -> Vec<Finding> {
    let Ok(path) = which::which("rbenv") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let version = run_extract(&path, &["--version"], r"rbenv (\S+)", timeout_ms).await;
    out.push(Finding {
        name: "rbenv".to_string(),
        category: Category::VersionManager,
        kind: FindingKind::VersionManager,
        status: Status::Installed,
        version,
        path: Some(path.clone()),
        detail: None,
    });
    for v in run_lines(&path, &["versions", "--bare"], timeout_ms).await {
        let v = v.trim().to_string();
        if v.is_empty() || v == "system" {
            continue;
        }
        out.push(Finding {
            name: "ruby".to_string(),
            category: Category::Language,
            kind: FindingKind::ManagedRuntime {
                manager: "rbenv".to_string(),
            },
            status: Status::Installed,
            version: Some(v),
            path: None,
            detail: None,
        });
    }
    out
}

async fn probe_asdf(timeout_ms: u64) -> Vec<Finding> {
    let Ok(path) = which::which("asdf") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let version = run_extract(&path, &["--version"], r"v?(\d+\.\d+\.\d+\S*)", timeout_ms).await;
    out.push(Finding {
        name: "asdf".to_string(),
        category: Category::VersionManager,
        kind: FindingKind::VersionManager,
        status: Status::Installed,
        version,
        path: Some(path.clone()),
        detail: None,
    });
    let lines = run_lines(&path, &["list"], timeout_ms).await;
    let mut current_plugin: Option<String> = None;
    for raw in lines {
        if raw.trim().is_empty() {
            continue;
        }
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            current_plugin = Some(raw.trim().to_string());
            continue;
        }
        let Some(plugin) = current_plugin.as_deref() else {
            continue;
        };
        let v = raw
            .trim()
            .trim_start_matches('*')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        if v.is_empty() {
            continue;
        }
        let (display_name, category) = match plugin {
            "nodejs" => ("node", Category::Language),
            "python" | "ruby" | "go" | "java" | "php" | "rust" | "deno" | "bun" | "lua" => {
                (plugin, Category::Language)
            }
            _ => (plugin, Category::Other),
        };
        out.push(Finding {
            name: display_name.to_string(),
            category,
            kind: FindingKind::ManagedRuntime {
                manager: "asdf".to_string(),
            },
            status: Status::Installed,
            version: Some(v),
            path: None,
            detail: None,
        });
    }
    out
}

async fn probe_conda(timeout_ms: u64) -> Vec<Finding> {
    let Ok(path) = which::which("conda") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let version = run_extract(&path, &["--version"], r"conda (\S+)", timeout_ms).await;
    out.push(Finding {
        name: "conda".to_string(),
        category: Category::VersionManager,
        kind: FindingKind::VersionManager,
        status: Status::Installed,
        version,
        path: Some(path.clone()),
        detail: None,
    });
    for line in run_lines(&path, &["env", "list"], timeout_ms).await {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let name = parts[0];
        let env_path = parts.last().copied().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        out.push(Finding {
            name: format!("conda:{}", name),
            category: Category::Language,
            kind: FindingKind::ManagedRuntime {
                manager: "conda".to_string(),
            },
            status: Status::Installed,
            version: None,
            path: if env_path.is_empty() {
                None
            } else {
                Some(PathBuf::from(env_path))
            },
            detail: None,
        });
    }
    out
}

async fn run_extract(path: &Path, args: &[&str], regex: &str, timeout_ms: u64) -> Option<String> {
    let fut = Command::new(path).args(args).output();
    let output = timeout(Duration::from_millis(timeout_ms), fut)
        .await
        .ok()?
        .ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let re = Regex::new(regex).ok()?;
    re.captures(&text)?.get(1).map(|m| m.as_str().to_string())
}

async fn run_lines(path: &Path, args: &[&str], timeout_ms: u64) -> Vec<String> {
    let fut = Command::new(path).args(args).output();
    let output = match timeout(Duration::from_millis(timeout_ms), fut).await {
        Ok(Ok(o)) => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}
