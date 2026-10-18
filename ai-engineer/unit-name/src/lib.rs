pub fn normalize(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();
    let stripped = if let Some(s) = lower.strip_suffix(".service") {
        s
    } else {
        &lower
    };

    if stripped == "ssh" {
        "sshd".to_string()
    } else {
        stripped.to_string()
    }
}

pub fn matches_unit(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

pub fn is_protected_unit(unit: &str, protected_list: &[&str]) -> bool {
    let norm = normalize(unit);
    protected_list.iter().any(|&p| normalize(p) == norm)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitName(String);

impl UnitName {
    pub fn new(raw: &str) -> Self {
        Self(normalize(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for UnitName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UnitName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
