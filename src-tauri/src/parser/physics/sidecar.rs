//! Types and helpers shared by the TotK helper-bone (BPHHB) and BotW
//! support-bone (BPHYSSB) sidecars.

use serde::Serialize;

/// One independent support/helper-bone motion graph. Driver bones provide the
/// input pose; driven bones receive the graph's translated or rotated output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarDriverGroup {
    pub index: usize,
    pub driver_indices: Vec<usize>,
    pub driver_bone_indices: Vec<usize>,
    pub driven_bone_indices: Vec<usize>,
    pub driver_bone_names: Vec<String>,
    pub driven_bone_names: Vec<String>,
    pub required_bone_indices: Vec<usize>,
    pub required_bone_names: Vec<String>,
    pub curve_count: usize,
    pub output_count: usize,
}

impl SidecarDriverGroup {
    pub fn display_name(&self) -> String {
        if self.driver_bone_names.is_empty() {
            format!("Driver group {}", self.index)
        } else {
            self.driver_bone_names.join(" + ")
        }
    }
}

/// Union-find over driver records; every output that mixes curves from two
/// drivers joins them into one group.
pub(crate) struct UnionFind {
    parents: Vec<usize>,
}

impl UnionFind {
    pub fn new(count: usize) -> Self {
        Self {
            parents: (0..count).collect(),
        }
    }
    pub fn find(&mut self, mut index: usize) -> usize {
        while self.parents[index] != index {
            self.parents[index] = self.parents[self.parents[index]];
            index = self.parents[index];
        }
        index
    }
    pub fn union(&mut self, left: usize, right: usize) {
        let left = self.find(left);
        let right = self.find(right);
        if left != right {
            self.parents[right] = left;
        }
    }
    /// Groups sorted by their lowest member, each sorted ascending.
    pub fn groups(&mut self) -> Vec<Vec<usize>> {
        let count = self.parents.len();
        let mut by_root: Vec<(usize, Vec<usize>)> = Vec::new();
        for index in 0..count {
            let root = self.find(index);
            match by_root.iter_mut().find(|(key, _)| *key == root) {
                Some((_, members)) => members.push(index),
                None => by_root.push((root, vec![index])),
            }
        }
        let mut groups: Vec<Vec<usize>> = by_root.into_iter().map(|(_, members)| members).collect();
        groups.sort_by_key(|group| group[0]);
        groups
    }
}

pub(crate) fn bone_name(names: &[String], index: i32) -> String {
    usize::try_from(index)
        .ok()
        .and_then(|index| names.get(index).cloned())
        .unwrap_or_else(|| format!("bone_{index}"))
}

pub(crate) fn distinct_non_negative(values: impl IntoIterator<Item = i32>) -> Vec<usize> {
    let mut result = Vec::new();
    for value in values {
        if let Ok(value) = usize::try_from(value) {
            if !result.contains(&value) {
                result.push(value);
            }
        }
    }
    result
}

/// Swaps recognised `L`/`R` side tokens: a lone L or R delimited by the start
/// or end of the name or by `_`, `:`, `-` or `.`.
pub fn swap_side_tokens(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let is_delimiter = |index: Option<usize>| match index {
        None => true,
        Some(index) => matches!(chars.get(index), Some('_' | ':' | '-' | '.')),
    };
    chars
        .iter()
        .enumerate()
        .map(|(index, character)| {
            let before = if index == 0 { None } else { Some(index - 1) };
            let after = if index + 1 < chars.len() {
                Some(index + 1)
            } else {
                None
            };
            let delimited = is_delimiter(before) && is_delimiter(after);
            match character {
                'L' if delimited => 'R',
                'R' if delimited => 'L',
                other => *other,
            }
        })
        .collect()
}

/// Reflects a rotation across the YZ plane: the axis loses its X component
/// and the angle flips, which is `(x, -y, -z, w)` once normalised.
pub fn mirror_quaternion_across_x(rotation: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = rotation;
    let length = (x * x + y * y + z * z + w * w).sqrt();
    if length <= 1.0e-8 || !length.is_finite() {
        return [x, -y, -z, w];
    }
    [x / length, -y / length, -z / length, w / length]
}

pub(crate) fn unique_bone_name(used: &[String], source_name: &str) -> String {
    let mut number = 1;
    loop {
        let candidate = format!("{source_name}_{number}");
        if !used.iter().any(|name| name == &candidate) {
            return candidate;
        }
        number += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_tokens_swap_only_when_delimited() {
        assert_eq!(swap_side_tokens("Arm_L"), "Arm_R");
        assert_eq!(swap_side_tokens("Link:Leg_R_1"), "Link:Leg_L_1");
        assert_eq!(swap_side_tokens("L"), "R");
        assert_eq!(swap_side_tokens("Skirt_LR"), "Skirt_LR");
        assert_eq!(swap_side_tokens("Lantern"), "Lantern");
        assert_eq!(swap_side_tokens("Hair.R"), "Hair.L");
    }

    #[test]
    fn mirrored_quaternion_keeps_x_and_flips_the_rest() {
        let mirrored = mirror_quaternion_across_x([0.0, 0.0, 0.0, 1.0]);
        assert_eq!(mirrored, [0.0, -0.0, -0.0, 1.0]);
        let mirrored = mirror_quaternion_across_x([0.5, 0.5, 0.5, 0.5]);
        assert_eq!(mirrored, [0.5, -0.5, -0.5, 0.5]);
    }

    #[test]
    fn union_find_groups_sort_by_lowest_member() {
        let mut sets = UnionFind::new(5);
        sets.union(3, 1);
        sets.union(4, 0);
        assert_eq!(sets.groups(), vec![vec![0, 4], vec![1, 3], vec![2]]);
    }
}
