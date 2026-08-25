#[derive(Debug, Clone, PartialEq)]
pub struct PackageRow {
    pub name: String,
    pub evr: String,
    pub arch: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequireEdge {
    pub pkg: String,
    pub dep: String,
    pub flags: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProvideEdge {
    pub pkg: String,
    pub provide: String,
    pub version: String,
}

#[derive(Debug, Clone, Default)]
pub struct PackageData {
    pub packages: Vec<PackageRow>,
    pub requires: Vec<RequireEdge>,
    pub provides: Vec<ProvideEdge>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitSnapshot {
    pub name: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub enabled: String,
}

impl UnitSnapshot {
    pub fn tuple(&self) -> (&str, &str, &str, &str) {
        (
            &self.load_state,
            &self.active_state,
            &self.sub_state,
            &self.enabled,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitChange {
    pub unit: String,
    pub kind: String,
    pub old_value: String,
    pub new_value: String,
}
