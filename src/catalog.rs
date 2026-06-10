use crate::model::{CatalogEntry, ServiceEntry};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

const BUILTIN: &str = include_str!("../catalog/builtin.toml");

#[derive(Debug, Deserialize)]
struct CatalogFile {
    #[serde(default)]
    tool: Vec<CatalogEntry>,
    #[serde(default)]
    service: Vec<ServiceEntry>,
}

#[derive(Debug, Default)]
pub struct LoadedCatalog {
    pub tools: Vec<CatalogEntry>,
    pub services: Vec<ServiceEntry>,
}

pub fn load_builtin() -> Result<LoadedCatalog> {
    let parsed: CatalogFile = toml::from_str(BUILTIN).context("failed to parse builtin catalog")?;
    Ok(LoadedCatalog {
        tools: parsed.tool,
        services: parsed.service,
    })
}

pub fn load_user(path: &Path) -> Result<LoadedCatalog> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read catalog {}", path.display()))?;
    let parsed: CatalogFile = toml::from_str(&content)
        .with_context(|| format!("failed to parse catalog {}", path.display()))?;
    Ok(LoadedCatalog {
        tools: parsed.tool,
        services: parsed.service,
    })
}

pub fn merge(base: LoadedCatalog, overlay: LoadedCatalog) -> LoadedCatalog {
    use std::collections::BTreeMap;

    let mut tools: BTreeMap<String, CatalogEntry> = base
        .tools
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect();
    for e in overlay.tools {
        tools.insert(e.name.clone(), e);
    }

    let mut services: BTreeMap<String, ServiceEntry> = base
        .services
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect();
    for e in overlay.services {
        services.insert(e.name.clone(), e);
    }

    LoadedCatalog {
        tools: tools.into_values().collect(),
        services: services.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use std::collections::BTreeSet;

    #[test]
    fn builtin_catalog_loads_and_has_unique_names() {
        let loaded = load_builtin().unwrap();

        assert!(loaded.tools.len() >= 100);
        assert!(loaded.services.len() >= 10);

        let mut tool_names = BTreeSet::new();
        for tool in &loaded.tools {
            assert!(
                tool_names.insert(&tool.name),
                "duplicate tool {}",
                tool.name
            );
        }

        let mut service_names = BTreeSet::new();
        for service in &loaded.services {
            assert!(
                service_names.insert(&service.name),
                "duplicate service {}",
                service.name
            );
        }
    }

    #[test]
    fn builtin_regexes_compile() {
        let loaded = load_builtin().unwrap();

        for tool in &loaded.tools {
            Regex::new(&tool.version_regex)
                .unwrap_or_else(|e| panic!("invalid version_regex for {}: {}", tool.name, e));

            if let Some(regex) = &tool.not_found_regex {
                Regex::new(regex)
                    .unwrap_or_else(|e| panic!("invalid not_found_regex for {}: {}", tool.name, e));
            }
        }
    }
}
