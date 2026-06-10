use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::{stream, StreamExt};
use std::collections::HashSet;
use std::path::PathBuf;

mod catalog;
mod model;
mod output;
mod probe;
mod self_update;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Inventory developer tools, services, and language runtimes with versions"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Output as JSON (suitable for piping into other tools)
    #[arg(long)]
    json: bool,

    /// Filter to specific categories, e.g. --category lang,db
    #[arg(long, value_delimiter = ',', value_parser = parse_category)]
    category: Vec<model::Category>,

    /// Probe only specific tool names, e.g. --only git,python
    #[arg(long, value_delimiter = ',')]
    only: Vec<String>,

    /// Show entries that were not found on this system
    #[arg(short, long)]
    verbose: bool,

    /// Skip service detection (launchctl / systemctl)
    #[arg(long)]
    no_services: bool,

    /// Skip version-manager probes (pyenv, nvm, asdf, rbenv, conda)
    #[arg(long)]
    no_version_managers: bool,

    /// Path to a custom catalog TOML file (overrides/extends builtin)
    #[arg(long)]
    catalog: Option<PathBuf>,

    /// Per-probe timeout in milliseconds
    #[arg(long, default_value_t = 2000, value_parser = parse_positive_u64)]
    timeout_ms: u64,

    /// Maximum number of binary probes running at the same time
    #[arg(long, default_value_t = 32, value_parser = parse_jobs)]
    jobs: usize,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Check whether a newer envprobe version is available
    CheckUpdate,
    /// Update envprobe from the latest GitHub release
    Update,
}

fn parse_category(value: &str) -> std::result::Result<model::Category, String> {
    model::Category::parse(value).ok_or_else(|| {
        format!(
            "unknown category {:?}; expected one of: {}",
            value,
            model::Category::accepted_values().join(", ")
        )
    })
}

fn parse_jobs(value: &str) -> std::result::Result<usize, String> {
    let jobs = value
        .parse::<usize>()
        .map_err(|_| format!("invalid jobs value {value:?}; expected a positive integer"))?;
    if jobs == 0 {
        Err("jobs must be greater than 0".to_string())
    } else {
        Ok(jobs)
    }
}

fn parse_positive_u64(value: &str) -> std::result::Result<u64, String> {
    let number = value
        .parse::<u64>()
        .map_err(|_| format!("invalid value {value:?}; expected a positive integer"))?;
    if number == 0 {
        Err("value must be greater than 0".to_string())
    } else {
        Ok(number)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        match command {
            Command::CheckUpdate => self_update::check_update()?,
            Command::Update => self_update::update()?,
        }
        return Ok(());
    }

    let mut loaded = catalog::load_builtin()?;
    if let Some(path) = &cli.catalog {
        let user_loaded = catalog::load_user(path)?;
        loaded = catalog::merge(loaded, user_loaded);
    }

    let wanted_names: HashSet<&str> = cli.only.iter().map(|s| s.as_str()).collect();
    let wanted_cats: Option<&[model::Category]> = if cli.category.is_empty() {
        None
    } else {
        Some(&cli.category)
    };

    let mut tools = loaded.tools;
    let mut services = loaded.services;

    if !cli.only.is_empty() {
        tools.retain(|e| wanted_names.contains(e.name.as_str()));
        services.retain(|e| wanted_names.contains(e.name.as_str()));
    }

    if let Some(cats) = wanted_cats {
        tools.retain(|e| cats.contains(&e.category));
        services.retain(|e| cats.contains(&e.category));
    }

    let binary_findings = stream::iter(tools.iter())
        .map(|e| probe::binary::probe(e, cli.timeout_ms))
        .buffer_unordered(cli.jobs)
        .collect::<Vec<_>>();

    let service_findings = async {
        if cli.no_services || services.is_empty() {
            Vec::new()
        } else {
            probe::service::probe_all(&services, cli.timeout_ms).await
        }
    };

    let vm_findings = async {
        if cli.no_version_managers || !version_managers_may_match(&wanted_names, wanted_cats) {
            Vec::new()
        } else {
            probe::version_mgr::probe_all(cli.timeout_ms).await
        }
    };

    let (mut binary, services, vms) = tokio::join!(binary_findings, service_findings, vm_findings);

    binary.extend(services);
    binary.extend(vms);
    let mut findings = binary;

    if let Some(cats) = wanted_cats {
        findings.retain(|f| cats.contains(&f.category));
    }
    if !cli.only.is_empty() {
        let want: std::collections::HashSet<&str> = cli.only.iter().map(|s| s.as_str()).collect();
        findings.retain(|f| want.contains(f.name.as_str()));
    }

    let host = model::HostInfo::detect();

    if cli.json {
        println!("{}", output::json::render(&host, &findings));
    } else {
        print!("{}", output::table::render(&host, &findings, cli.verbose));
    }

    Ok(())
}

fn version_managers_may_match(
    wanted_names: &HashSet<&str>,
    wanted_cats: Option<&[model::Category]>,
) -> bool {
    let category_match = match wanted_cats {
        None => true,
        Some(cats) => cats.iter().any(|cat| {
            matches!(
                cat,
                model::Category::VersionManager
                    | model::Category::Language
                    | model::Category::Other
            )
        }),
    };

    if !category_match {
        return false;
    }

    wanted_names.is_empty()
        || wanted_names.iter().any(|name| {
            matches!(
                *name,
                "pyenv"
                    | "nvm"
                    | "rbenv"
                    | "asdf"
                    | "conda"
                    | "python"
                    | "node"
                    | "ruby"
                    | "go"
                    | "java"
                    | "php"
                    | "rust"
                    | "deno"
                    | "bun"
                    | "lua"
            ) || name.starts_with("conda:")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn parses_category_aliases() {
        let cli = Cli::try_parse_from(["envprobe", "--category", "lang,db"]).unwrap();

        assert_eq!(
            cli.category,
            vec![model::Category::Language, model::Category::Database]
        );
    }

    #[test]
    fn rejects_unknown_category() {
        let err = Cli::try_parse_from(["envprobe", "--category", "lang,nope"]).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn parses_check_update_subcommand() {
        let cli = Cli::try_parse_from(["envprobe", "check-update"]).unwrap();

        assert!(matches!(cli.command, Some(Command::CheckUpdate)));
    }

    #[test]
    fn parses_jobs_option() {
        let cli = Cli::try_parse_from(["envprobe", "--jobs", "8"]).unwrap();

        assert_eq!(cli.jobs, 8);
    }

    #[test]
    fn rejects_zero_jobs() {
        let err = Cli::try_parse_from(["envprobe", "--jobs", "0"]).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn rejects_zero_timeout() {
        let err = Cli::try_parse_from(["envprobe", "--timeout-ms", "0"]).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn skips_version_managers_for_unrelated_only_filter() {
        let wanted_names = HashSet::from(["git"]);

        assert!(!version_managers_may_match(&wanted_names, None));
    }

    #[test]
    fn keeps_version_managers_for_managed_runtime_filter() {
        let wanted_names = HashSet::from(["python"]);

        assert!(version_managers_may_match(&wanted_names, None));
    }
}
