//! Transitive systemd unit dependency graph (Phase 1 / TODO 6).
//!
//! Provides read-only analysis of systemd unit relationships to detect whether stopping
//! or modifying a given unit would transitively stop or impact protected units (e.g.,
//! ssh, dbus, firewalld, systemd-udevd).

use std::collections::{HashMap, HashSet, VecDeque};

/// Describes the dependency relationship between two systemd units.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnitDependencies {
    /// Units that this unit strongly requires (Requires=).
    pub requires: Vec<String>,
    /// Units that strongly require this unit (RequiredBy=).
    pub required_by: Vec<String>,
    /// Units that this unit weakly wants (Wants=).
    pub wants: Vec<String>,
    /// Units that order after this unit (After=).
    pub after: Vec<String>,
    /// Units that order before this unit (Before=).
    pub before: Vec<String>,
}

/// Represents an impact on a protected unit if a candidate unit is stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedUnitImpact {
    /// The protected unit that would be affected.
    pub protected_unit: String,
    /// The transitive chain of Requires dependencies leading from the candidate unit to the protected unit.
    pub dependency_chain: Vec<String>,
    /// Explanation of why the unit is impacted.
    pub reason: String,
}

/// In-memory graph of unit dependencies for impact analysis.
#[derive(Debug, Default, Clone)]
pub struct UnitDependencyGraph {
    units: HashMap<String, UnitDependencies>,
}

impl UnitDependencyGraph {
    /// Creates an empty dependency graph.
    pub fn new() -> Self {
        Self {
            units: HashMap::new(),
        }
    }

    /// Normalizes unit names by lowercasing and stripping whitespace.
    pub fn normalize_unit_name(raw: &str) -> String {
        let trimmed = raw.trim();
        let name = if let Some(stripped) = trimmed.strip_suffix(".service") {
            stripped
        } else {
            trimmed
        };
        name.to_lowercase()
    }

    /// Registers a unit and its parsed dependencies in the graph.
    pub fn add_unit(&mut self, unit: &str, deps: UnitDependencies) {
        let norm_unit = Self::normalize_unit_name(unit);
        self.units.insert(norm_unit, deps);
    }

    /// Parses key-value output from `systemctl show <unit> --property=Requires,RequiredBy,Wants,After,Before`.
    pub fn parse_systemctl_show(output: &str) -> UnitDependencies {
        let mut deps = UnitDependencies::default();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let items: Vec<String> = v
                    .split_whitespace()
                    .map(Self::normalize_unit_name)
                    .filter(|s| !s.is_empty())
                    .collect();
                match k.trim() {
                    "Requires" => deps.requires = items,
                    "RequiredBy" => deps.required_by = items,
                    "Wants" => deps.wants = items,
                    "After" => deps.after = items,
                    "Before" => deps.before = items,
                    _ => {}
                }
            }
        }
        deps
    }

    /// Analyzes the transitive shutdown impact if `unit` is stopped.
    ///
    /// When unit `U` is stopped, systemd transitively stops any unit `V` where `V` Requires `U`
    /// (i.e. `U` is in `V.requires` or `V` is in `U.required_by`).
    /// This method computes the transitive closure and flags any protected unit that would be stopped.
    pub fn detect_protected_unit_impact(
        &self,
        candidate_unit: &str,
        protected_units: &[&str],
    ) -> Vec<ProtectedUnitImpact> {
        let root = Self::normalize_unit_name(candidate_unit);
        let normalized_protected: HashSet<String> = protected_units
            .iter()
            .map(|u| Self::normalize_unit_name(u))
            .collect();

        let mut impacts = Vec::new();

        // If the candidate unit itself IS a protected unit:
        if normalized_protected.contains(&root) {
            impacts.push(ProtectedUnitImpact {
                protected_unit: root.clone(),
                dependency_chain: vec![root.clone()],
                reason: format!("Direct stop of protected unit '{root}'"),
            });
        }

        // Transitive traversal: queue of (current_unit, chain)
        let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();

        queue.push_back((root.clone(), vec![root.clone()]));
        visited.insert(root.clone());

        while let Some((curr, chain)) = queue.pop_front() {
            // Find all units that depend on `curr` via Requires
            // 1. From curr's required_by field
            let mut dependent_units = Vec::new();
            if let Some(deps) = self.units.get(&curr) {
                dependent_units.extend(deps.required_by.clone());
            }

            // 2. Also check inverted: any registered unit whose `requires` includes `curr`
            for (unit_name, unit_deps) in &self.units {
                if unit_deps.requires.iter().any(|r| r == &curr)
                    && !dependent_units.contains(unit_name)
                {
                    dependent_units.push(unit_name.clone());
                }
            }

            for dep in dependent_units {
                if visited.insert(dep.clone()) {
                    let mut next_chain = chain.clone();
                    next_chain.push(dep.clone());

                    if normalized_protected.contains(&dep) {
                        impacts.push(ProtectedUnitImpact {
                            protected_unit: dep.clone(),
                            dependency_chain: next_chain.clone(),
                            reason: format!(
                                "Stopping '{}' transitively stops protected unit '{}' via chain: {}",
                                root,
                                dep,
                                next_chain.join(" -> ")
                            ),
                        });
                    }

                    queue.push_back((dep, next_chain));
                }
            }
        }

        impacts
    }
}
