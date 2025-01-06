use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub struct TotkbitsVersion {
    pub major: i32,
    pub mid: i32,
    pub low: i32,
}

impl TotkbitsVersion {
    pub fn from_str(version: &str) -> Self {
        let mut ver = version.to_string();
        ver.retain(|c| c.is_numeric() || c == '.');
        if ver.matches('.').count() != 2 {
            return TotkbitsVersion::default();
        }
        let mut parts = ver.split('.').map(|x| x.parse::<i32>().unwrap_or(-1));
        TotkbitsVersion {
            major: parts.next().unwrap_or(-1),
            mid: parts.next().unwrap_or(-1),
            low: parts.next().unwrap_or(-1),
        }
    }
    pub fn is_valid(&self) -> bool {
        self.major >= 0 && self.mid >= 0 && self.low >= 0
    }
    pub fn as_str(&self) -> String {
        format!("{}.{}.{}", self.major, self.mid, self.low)
    }
}

impl Default for TotkbitsVersion {
    fn default() -> Self {
        TotkbitsVersion {
            major: -1,
            mid: -1,
            low: -1,
        }
    }
}

impl PartialOrd for TotkbitsVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TotkbitsVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare major fields first
        self.major
            .cmp(&other.major)
            // If major fields are equal, compare mid fields
            .then(self.mid.cmp(&other.mid))
            // If mid fields are also equal, compare low fields
            .then(self.low.cmp(&other.low))
    }
}
