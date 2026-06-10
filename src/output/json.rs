use crate::model::{Finding, HostInfo, Report};

pub fn render(host: &HostInfo, findings: &[Finding]) -> String {
    let report = Report {
        host: host.clone(),
        findings,
    };
    serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
}
