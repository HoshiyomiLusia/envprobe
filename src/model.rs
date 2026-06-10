use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Vcs,
    Language,
    Container,
    WebServer,
    Database,
    BuildTool,
    PackageManager,
    #[serde(rename = "devops")]
    DevOps,
    VersionManager,
    Service,
    Utility,
    Other,
}

impl Category {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "vcs" => Some(Self::Vcs),
            "language" | "lang" => Some(Self::Language),
            "container" => Some(Self::Container),
            "web-server" | "web" => Some(Self::WebServer),
            "database" | "db" => Some(Self::Database),
            "build-tool" | "build" => Some(Self::BuildTool),
            "package-manager" | "pkg" => Some(Self::PackageManager),
            "devops" => Some(Self::DevOps),
            "version-manager" | "vm" => Some(Self::VersionManager),
            "service" => Some(Self::Service),
            "utility" | "util" => Some(Self::Utility),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    pub fn accepted_values() -> &'static [&'static str] {
        &[
            "vcs",
            "language",
            "lang",
            "container",
            "web-server",
            "web",
            "database",
            "db",
            "build-tool",
            "build",
            "package-manager",
            "pkg",
            "devops",
            "version-manager",
            "vm",
            "service",
            "utility",
            "util",
            "other",
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Vcs => "VCS",
            Self::Language => "Language",
            Self::Container => "Container",
            Self::WebServer => "Web Server",
            Self::Database => "Database",
            Self::BuildTool => "Build Tool",
            Self::PackageManager => "Package Manager",
            Self::DevOps => "DevOps",
            Self::VersionManager => "Version Manager",
            Self::Service => "Service",
            Self::Utility => "Utility",
            Self::Other => "Other",
        }
    }

    pub fn display_order(&self) -> u8 {
        match self {
            Self::Vcs => 0,
            Self::Language => 1,
            Self::VersionManager => 2,
            Self::PackageManager => 3,
            Self::BuildTool => 4,
            Self::Container => 5,
            Self::WebServer => 6,
            Self::Database => 7,
            Self::DevOps => 8,
            Self::Utility => 9,
            Self::Service => 10,
            Self::Other => 11,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub name: String,
    pub command: String,
    #[serde(default = "default_args")]
    pub args: Vec<String>,
    #[serde(default = "default_stream")]
    pub stream: Stream,
    pub version_regex: String,
    #[serde(default)]
    pub not_found_regex: Option<String>,
    pub category: Category,
}

fn default_args() -> Vec<String> {
    vec!["--version".to_string()]
}

fn default_stream() -> Stream {
    Stream::Stdout
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEntry {
    pub name: String,
    pub category: Category,
    /// Substring match against labels in `launchctl list` output (macOS).
    #[serde(default)]
    pub macos_match: Option<String>,
    /// Candidate systemd unit names tried in order (Linux).
    #[serde(default)]
    pub linux_units: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub name: String,
    pub category: Category,
    pub kind: FindingKind,
    pub status: Status,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FindingKind {
    Binary,
    // Built only by the unix service probes; unused on other targets.
    #[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
    Service {
        running: bool,
    },
    VersionManager,
    ManagedRuntime {
        manager: String,
    },
}

impl FindingKind {
    pub fn sort_order(&self) -> u8 {
        match self {
            Self::Binary => 0,
            Self::Service { .. } => 1,
            Self::VersionManager => 0,
            Self::ManagedRuntime { .. } => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Installed,
    NotFound,
    ProbeError,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    pub hostname: String,
}

impl HostInfo {
    pub fn detect() -> Self {
        let hostname = hostname_string();
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            hostname,
        }
    }
}

fn hostname_string() -> String {
    if let Ok(name) = std::env::var("HOSTNAME") {
        if !name.is_empty() {
            return name;
        }
    }
    if let Ok(output) = std::process::Command::new("hostname").output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    "unknown".to_string()
}

#[derive(Debug, Serialize)]
pub struct Report<'a> {
    pub host: HostInfo,
    pub findings: &'a [Finding],
}
