use crate::model::{Finding, FindingKind, HostInfo, Status};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use std::collections::BTreeMap;

pub fn render(host: &HostInfo, findings: &[Finding], verbose: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "envprobe — {} {} on {}\n\n",
        host.os, host.arch, host.hostname
    ));

    let mut by_category: BTreeMap<(u8, &'static str), Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        if !verbose && f.status == Status::NotFound {
            continue;
        }
        by_category
            .entry((f.category.display_order(), f.category.label()))
            .or_default()
            .push(f);
    }

    if by_category.is_empty() {
        out.push_str("No tools detected.\n");
        return out;
    }

    for ((_, label), mut items) in by_category {
        items.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.kind.sort_order().cmp(&b.kind.sort_order()))
                .then_with(|| b.version.cmp(&a.version))
        });

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec![label, "Version", "Source"]);

        for f in &items {
            table.add_row(vec![Cell::new(&f.name), version_cell(f), source_cell(f)]);
        }
        out.push_str(&format!("{}\n\n", table));
    }
    out
}

fn version_cell(f: &Finding) -> Cell {
    match (&f.kind, f.status, f.version.as_deref()) {
        (FindingKind::Service { running: true }, _, _) => Cell::new("running").fg(Color::Green),
        (FindingKind::Service { running: false }, _, _) => Cell::new("stopped").fg(Color::DarkGrey),
        (_, Status::Installed, Some(v)) => Cell::new(v).fg(Color::Green),
        (_, Status::Installed, None) => Cell::new("installed").fg(Color::Green),
        (_, Status::NotFound, _) => Cell::new("not found").fg(Color::DarkGrey),
        (_, Status::ProbeError, _) => Cell::new("error").fg(Color::Yellow),
    }
}

fn source_cell(f: &Finding) -> Cell {
    let text = match &f.kind {
        FindingKind::Binary => f
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        FindingKind::Service { .. } => f.detail.clone().unwrap_or_else(|| {
            f.path
                .as_ref()
                .map(|p| format!("service: {}", p.display()))
                .unwrap_or_else(|| "service".to_string())
        }),
        FindingKind::VersionManager => f
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        FindingKind::ManagedRuntime { manager } => format!("via {}", manager),
    };
    Cell::new(text)
}
