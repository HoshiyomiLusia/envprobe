use crate::model::{CatalogEntry, Finding, FindingKind, Status, Stream};
use regex::Regex;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub async fn probe(entry: &CatalogEntry, timeout_ms: u64) -> Finding {
    let path = match which::which(&entry.command) {
        Ok(p) => p,
        Err(_) => {
            return Finding {
                name: entry.name.clone(),
                category: entry.category.clone(),
                kind: FindingKind::Binary,
                status: Status::NotFound,
                version: None,
                path: None,
                detail: None,
            };
        }
    };

    let mut cmd = Command::new(&path);
    cmd.args(&entry.args);
    cmd.kill_on_drop(true);

    let exec = cmd.output();
    let output = match timeout(Duration::from_millis(timeout_ms), exec).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return Finding {
                name: entry.name.clone(),
                category: entry.category.clone(),
                kind: FindingKind::Binary,
                status: Status::ProbeError,
                version: None,
                path: Some(path),
                detail: Some(format!("spawn failed: {}", e)),
            };
        }
        Err(_) => {
            return Finding {
                name: entry.name.clone(),
                category: entry.category.clone(),
                kind: FindingKind::Binary,
                status: Status::ProbeError,
                version: None,
                path: Some(path),
                detail: Some(format!("timed out after {}ms", timeout_ms)),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let haystack: String = match entry.stream {
        Stream::Stdout => stdout.into_owned(),
        Stream::Stderr => stderr.into_owned(),
        Stream::Both => format!("{}\n{}", stdout, stderr),
    };

    if let Some(not_found_regex) = &entry.not_found_regex {
        let re = match Regex::new(not_found_regex) {
            Ok(re) => re,
            Err(e) => {
                return Finding {
                    name: entry.name.clone(),
                    category: entry.category.clone(),
                    kind: FindingKind::Binary,
                    status: Status::ProbeError,
                    version: None,
                    path: Some(path),
                    detail: Some(format!("invalid not_found_regex: {}", e)),
                };
            }
        };
        if re.is_match(&haystack) {
            return Finding {
                name: entry.name.clone(),
                category: entry.category.clone(),
                kind: FindingKind::Binary,
                status: Status::NotFound,
                version: None,
                path: Some(path),
                detail: Some(format!(
                    "not_found_regex {:?} matched output",
                    not_found_regex
                )),
            };
        }
    }

    let re = match Regex::new(&entry.version_regex) {
        Ok(re) => re,
        Err(e) => {
            return Finding {
                name: entry.name.clone(),
                category: entry.category.clone(),
                kind: FindingKind::Binary,
                status: Status::ProbeError,
                version: None,
                path: Some(path),
                detail: Some(format!("invalid version_regex: {}", e)),
            };
        }
    };
    let version = re
        .captures(&haystack)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let (status, detail) = if version.is_some() {
        (Status::Installed, None)
    } else {
        (
            Status::ProbeError,
            Some(format!(
                "version regex {:?} did not match output: {:?}",
                entry.version_regex,
                haystack.trim()
            )),
        )
    };

    Finding {
        name: entry.name.clone(),
        category: entry.category.clone(),
        kind: FindingKind::Binary,
        status,
        version,
        path: Some(path),
        detail,
    }
}
