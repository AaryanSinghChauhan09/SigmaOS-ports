//! SigmaOS ports — ports tree manager.
//!
//! Manages a SigmaOS ports collection: categorized entries with version,
//! dependencies, and build state. JSON-backed database.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Errors produced by this component.
#[derive(Debug, thiserror::Error)]
pub enum PortsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("port not found: {0}")]
    NotFound(String),
    #[error("category not found: {0}")]
    CategoryNotFound(String),
    #[error("dependency cycle involving: {0}")]
    Cycle(String),
    #[error("validation error: {0}")]
    Validation(String),
}

/// Build/install state of a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortState {
    /// Not yet built.
    Available,
    /// Currently building.
    Building,
    /// Built and installed.
    Installed,
    /// Build failed.
    Failed,
}

/// A single port entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub category: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub license: String,
    #[serde(default = "default_state")]
    pub state: PortState,
    #[serde(default)]
    pub maintainer: String,
}

fn default_state() -> PortState {
    PortState::Available
}

/// Ports tree database.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PortsDb {
    #[serde(default)]
    pub ports: BTreeMap<String, Port>,
    #[serde(default)]
    pub categories: BTreeSet<String>,
}

impl PortsDb {
    /// Add a port.
    pub fn add(&mut self, port: Port) -> Result<(), PortsError> {
        if port.name.is_empty() {
            return Err(PortsError::Validation("empty port name".into()));
        }
        self.categories.insert(port.category.clone());
        self.ports.insert(port.name.clone(), port);
        Ok(())
    }

    /// Get a port by name.
    pub fn get(&self, name: &str) -> Option<&Port> {
        self.ports.get(name)
    }

    /// Ports in a category.
    pub fn in_category(&self, cat: &str) -> Vec<&Port> {
        self.ports.values().filter(|p| p.category == cat).collect()
    }

    /// List all categories.
    pub fn list_categories(&self) -> Vec<&String> {
        self.categories.iter().collect()
    }

    /// Dependency-ordered build plan for a target port.
    pub fn build_plan(&self, target: &str) -> Result<Vec<String>, PortsError> {
        let mut order = Vec::new();
        let mut visited = BTreeSet::new();
        let mut stack = BTreeSet::new();
        self.visit(target, &mut visited, &mut stack, &mut order)?;
        Ok(order)
    }

    fn visit(
        &self,
        name: &str,
        visited: &mut BTreeSet<String>,
        stack: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), PortsError> {
        if visited.contains(name) {
            return Ok(());
        }
        if !stack.insert(name.to_string()) {
            return Err(PortsError::Cycle(name.into()));
        }
        let port = self
            .ports
            .get(name)
            .ok_or_else(|| PortsError::NotFound(name.into()))?;
        for dep in &port.depends {
            self.visit(dep, visited, stack, order)?;
        }
        stack.remove(name);
        visited.insert(name.to_string());
        order.push(name.to_string());
        Ok(())
    }

    /// Mark a port state.
    pub fn set_state(&mut self, name: &str, state: PortState) -> Result<(), PortsError> {
        let p = self
            .ports
            .get_mut(name)
            .ok_or_else(|| PortsError::NotFound(name.into()))?;
        p.state = state;
        Ok(())
    }

    /// Ports in a given state.
    pub fn by_state(&self, state: PortState) -> Vec<&Port> {
        self.ports.values().filter(|p| p.state == state).collect()
    }

    /// Search ports by name or description.
    pub fn search(&self, query: &str) -> Vec<&Port> {
        self.ports
            .values()
            .filter(|p| {
                p.name.contains(query)
                    || p.description.to_lowercase().contains(&query.to_lowercase())
            })
            .collect()
    }

    /// Save to JSON.
    pub fn save(&self, path: &Path) -> Result<(), PortsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load from JSON.
    pub fn load(path: &Path) -> Result<Self, PortsError> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| PortsError::NotFound(path.display().to_string()))?;
        Ok(serde_json::from_str(&text)?)
    }
}

/// Default ports database path.
pub fn default_db_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("org", "sigmaos", "sigmaos")
        .map(|p| p.data_dir().join("ports").join("db.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(name: &str, cat: &str, deps: &[&str]) -> Port {
        Port {
            name: name.into(),
            category: cat.into(),
            version: "1.0".into(),
            description: format!("{} port", name),
            depends: deps.iter().map(|s| s.to_string()).collect(),
            license: String::new(),
            state: PortState::Available,
            maintainer: String::new(),
        }
    }

    #[test]
    fn add_get_and_categories() {
        let mut db = PortsDb::default();
        db.add(port("bash", "shells", &[])).unwrap();
        db.add(port("git", "vcs", &[])).unwrap();
        assert!(db.get("bash").is_some());
        assert!(db.get("nope").is_none());
        assert_eq!(db.in_category("shells").len(), 1);
        assert_eq!(db.list_categories().len(), 2);
        assert!(matches!(
            db.add(port("", "x", &[])),
            Err(PortsError::Validation(_))
        ));
    }

    #[test]
    fn build_plan_orders_dependencies() {
        let mut db = PortsDb::default();
        db.add(port("a", "c", &[])).unwrap();
        db.add(port("b", "c", &["a"])).unwrap();
        db.add(port("c", "c", &["b"])).unwrap();
        let plan = db.build_plan("c").unwrap();
        assert_eq!(plan, vec!["a", "b", "c"]);
    }

    #[test]
    fn build_plan_detects_cycle_and_missing() {
        let mut db = PortsDb::default();
        db.add(port("x", "c", &["y"])).unwrap();
        db.add(port("y", "c", &["x"])).unwrap();
        assert!(matches!(db.build_plan("x"), Err(PortsError::Cycle(_))));
        assert!(matches!(
            db.build_plan("ghost"),
            Err(PortsError::NotFound(_))
        ));
    }

    #[test]
    fn state_and_search() {
        let mut db = PortsDb::default();
        db.add(port("firefox", "net", &[])).unwrap();
        db.add(port("git", "vcs", &[])).unwrap();
        db.set_state("git", PortState::Installed).unwrap();
        assert!(matches!(
            db.set_state("ghost", PortState::Building),
            Err(PortsError::NotFound(_))
        ));
        assert_eq!(db.by_state(PortState::Installed).len(), 1);
        assert_eq!(db.by_state(PortState::Available).len(), 1);
        assert_eq!(db.search("FIRE").len(), 1);
        assert_eq!(db.search("port").len(), 2);
        assert!(db.search("zzz").is_empty());
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/db.json");
        let mut db = PortsDb::default();
        db.add(port("vim", "editors", &["ncurses"])).unwrap();
        db.add(port("ncurses", "libs", &[])).unwrap();
        db.set_state("vim", PortState::Building).unwrap();
        db.save(&path).unwrap();
        let loaded = PortsDb::load(&path).unwrap();
        assert_eq!(loaded.ports.len(), 2);
        assert_eq!(loaded.get("vim").unwrap().state, PortState::Building);
        assert_eq!(loaded.build_plan("vim").unwrap(), vec!["ncurses", "vim"]);
        assert!(PortsDb::load(&dir.path().join("missing.json")).is_err());
    }
}
