use crate::model::{Finding, ServiceEntry};

#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::model::{FindingKind, Status};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::time::Duration;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use tokio::{process::Command, time::timeout};

pub async fn probe_all(entries: &[ServiceEntry], timeout_ms: u64) -> Vec<Finding> {
    #[cfg(target_os = "macos")]
    {
        return probe_macos(entries, timeout_ms).await;
    }
    #[cfg(target_os = "linux")]
    {
        return probe_linux(entries, timeout_ms).await;
    }
    #[allow(unreachable_code)]
    {
        let _ = (entries, timeout_ms);
        Vec::new()
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn not_found(entry: &ServiceEntry) -> Finding {
    Finding {
        name: entry.name.clone(),
        category: entry.category.clone(),
        kind: FindingKind::Service { running: false },
        status: Status::NotFound,
        version: None,
        path: None,
        detail: None,
    }
}

#[cfg(target_os = "macos")]
async fn probe_macos(entries: &[ServiceEntry], timeout_ms: u64) -> Vec<Finding> {
    use std::collections::HashMap;

    // launchctl unavailable: report no services.
    let Ok(map): Result<HashMap<String, bool>, _> = list_launchctl(timeout_ms).await else {
        return Vec::new();
    };

    let mut findings = Vec::new();
    for entry in entries {
        let matches: Vec<(&String, bool)> = match entry.macos_match.as_deref() {
            Some(needle) => map
                .iter()
                .filter(|(label, _)| label.contains(needle))
                .map(|(label, running)| (label, *running))
                .collect(),
            None => Vec::new(),
        };

        if matches.is_empty() {
            findings.push(not_found(entry));
            continue;
        }

        for (label, running) in matches {
            findings.push(Finding {
                name: entry.name.clone(),
                category: entry.category.clone(),
                kind: FindingKind::Service { running },
                status: Status::Installed,
                version: None,
                path: None,
                detail: Some(format!("launchctl: {}", label)),
            });
        }
    }
    findings
}

#[cfg(target_os = "macos")]
async fn list_launchctl(
    timeout_ms: u64,
) -> anyhow::Result<std::collections::HashMap<String, bool>> {
    use std::collections::HashMap;

    let fut = Command::new("launchctl").arg("list").output();
    let output = timeout(Duration::from_millis(timeout_ms), fut).await??;
    anyhow::ensure!(
        output.status.success(),
        "launchctl exited with {}",
        output.status
    );

    let text = String::from_utf8_lossy(&output.stdout);
    let mut map = HashMap::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        let pid = cols[0];
        let label = cols[2].to_string();
        let running = pid != "-" && !pid.is_empty();
        map.insert(label, running);
    }
    Ok(map)
}

#[cfg(target_os = "linux")]
async fn probe_linux(entries: &[ServiceEntry], timeout_ms: u64) -> Vec<Finding> {
    // No usable systemd (absent, or not booted as PID 1 in a container): no services.
    if which::which("systemctl").is_err() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for entry in entries {
        let mut matched = false;
        for unit in &entry.linux_units {
            match query_systemd(unit, timeout_ms).await {
                SystemdState::Unavailable => return Vec::new(),
                SystemdState::Active(active) => {
                    findings.push(Finding {
                        name: entry.name.clone(),
                        category: entry.category.clone(),
                        kind: FindingKind::Service { running: active },
                        status: Status::Installed,
                        version: None,
                        path: None,
                        detail: Some(format!("systemd: {}", unit)),
                    });
                    matched = true;
                    break;
                }
                SystemdState::NotInstalled => continue,
            }
        }
        if !matched {
            findings.push(not_found(entry));
        }
    }
    findings
}

#[cfg(target_os = "linux")]
enum SystemdState {
    Active(bool),
    NotInstalled,
    Unavailable,
}

#[cfg(target_os = "linux")]
async fn query_systemd(unit: &str, timeout_ms: u64) -> SystemdState {
    let fut = Command::new("systemctl")
        .args([
            "show",
            unit,
            "--property=LoadState",
            "--property=ActiveState",
            "--value",
        ])
        .output();
    let output = match timeout(Duration::from_millis(timeout_ms), fut).await {
        Ok(Ok(o)) => o,
        Ok(Err(_)) => return SystemdState::Unavailable,
        Err(_) => return SystemdState::NotInstalled, // timeout: skip this unit
    };
    // Unknown units still exit 0 (LoadState=not-found); a non-zero exit means
    // systemd itself is unreachable.
    if !output.status.success() {
        return SystemdState::Unavailable;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return SystemdState::NotInstalled;
    }
    let load = lines[0].trim();
    let active = lines[1].trim();
    if load == "not-found" || load.is_empty() {
        SystemdState::NotInstalled
    } else {
        SystemdState::Active(active == "active")
    }
}
