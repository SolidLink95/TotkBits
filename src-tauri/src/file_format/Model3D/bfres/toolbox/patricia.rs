//! Port of Syroot's `ResDict.UpdateNodes` / `ResDictUpdate.Tree` (the
//! BigInteger based Patricia trie builder the Toolbox DLL runs on every
//! dictionary before writing it). Keys are treated as big-endian integers of
//! their UTF-8 bytes; bit indices count from the least significant bit.

#[derive(Clone, Debug)]
pub struct DictNode {
    pub reference: u32,
    pub left: u16,
    pub right: u16,
    /// `None` for the root node (Toolbox writes an empty string for it).
    pub key: Option<String>,
}

/// A non-negative big integer stored as big-endian bytes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Big(Vec<u8>);

impl Big {
    fn zero() -> Self {
        Big(Vec::new())
    }

    fn from_key(key: &str) -> Self {
        // BigInteger built from the bit string of the UTF-8 bytes: the first
        // byte is the most significant one.
        let mut bytes = key.as_bytes().to_vec();
        while bytes.first() == Some(&0) {
            bytes.remove(0);
        }
        Big(bytes)
    }

    fn bit(&self, index: i64) -> u32 {
        if index < 0 {
            // `n >> (int)(b & 0xffffffff)` with b == -1 shifts left by one, so
            // the lowest bit is always clear.
            return 0;
        }
        let byte = (index / 8) as usize;
        if byte >= self.0.len() {
            return 0;
        }
        let value = self.0[self.0.len() - 1 - byte];
        u32::from((value >> (index % 8)) & 1)
    }

    fn bit_length(&self) -> i64 {
        // BitLength counts the halvings until zero, plus one; zero yields 1.
        let mut top = None;
        for (i, byte) in self.0.iter().enumerate() {
            if *byte != 0 {
                let bits_in_top = 8 - byte.leading_zeros() as i64;
                top = Some((self.0.len() - 1 - i) as i64 * 8 + bits_in_top);
                break;
            }
        }
        top.unwrap_or(1)
    }

    fn first_1bit(&self) -> i64 {
        let len = self.bit_length();
        for i in 0..len {
            if self.bit(i) == 1 {
                return i;
            }
        }
        // Toolbox throws here; only reachable for an all-zero key.
        0
    }

    fn bit_mismatch(&self, other: &Big) -> i64 {
        let len = self.bit_length().max(other.bit_length());
        for i in 0..len {
            if self.bit(i) != other.bit(i) {
                return i;
            }
        }
        -1
    }

    fn name(&self) -> String {
        // `Data.ToByteArray()` reversed; a set top bit adds a leading zero byte.
        let mut bytes = self.0.clone();
        if bytes.first().is_some_and(|b| b & 0x80 != 0) {
            bytes.insert(0, 0);
        }
        if bytes.is_empty() {
            bytes.push(0);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

#[derive(Clone, Debug)]
struct TreeNode {
    child: [usize; 2],
    parent: usize,
    bit_index: i64,
    data: Big,
}

struct Tree {
    nodes: Vec<TreeNode>,
    /// Entry order of `Tree.entries`: (data, node) in insertion order.
    entries: Vec<(Big, usize)>,
}

impl Tree {
    const ROOT: usize = 0;

    fn new() -> Self {
        let root = TreeNode {
            child: [Self::ROOT, Self::ROOT],
            parent: Self::ROOT,
            bit_index: -1,
            data: Big::zero(),
        };
        let mut tree = Tree {
            nodes: vec![root],
            entries: Vec::new(),
        };
        tree.insert_entry(Big::zero(), Self::ROOT);
        tree
    }

    fn insert_entry(&mut self, data: Big, node: usize) {
        if let Some(existing) = self.entries.iter_mut().find(|(key, _)| *key == data) {
            existing.1 = node;
        } else {
            self.entries.push((data, node));
        }
    }

    fn entry_index(&self, data: &Big) -> usize {
        self.entries
            .iter()
            .position(|(key, _)| key == data)
            .unwrap_or(0)
    }

    fn new_node(&mut self, data: Big, bit_index: i64, parent: usize) -> usize {
        let index = self.nodes.len();
        self.nodes.push(TreeNode {
            child: [index, index],
            parent,
            bit_index,
            data,
        });
        index
    }

    fn search(&self, data: &Big, prev: bool) -> usize {
        if self.nodes[Self::ROOT].child[0] == Self::ROOT {
            return Self::ROOT;
        }
        let mut node = self.nodes[Self::ROOT].child[0];
        let mut prev_node = node;
        loop {
            prev_node = node;
            let n = &self.nodes[node];
            node = n.child[data.bit(n.bit_index) as usize];
            if self.nodes[node].bit_index <= self.nodes[prev_node].bit_index {
                break;
            }
        }
        if prev {
            prev_node
        } else {
            node
        }
    }

    fn insert(&mut self, name: &str) {
        let data = Big::from_key(name);
        let mut current = self.search(&data, true);
        let mut bit_idx = self.nodes[current].data.bit_mismatch(&data);
        while bit_idx < self.nodes[self.nodes[current].parent].bit_index {
            current = self.nodes[current].parent;
        }
        let current_bit = self.nodes[current].bit_index;
        if bit_idx < current_bit {
            let parent = self.nodes[current].parent;
            let new_node = self.new_node(data.clone(), bit_idx, parent);
            let dir = data.bit(bit_idx) as usize;
            self.nodes[new_node].child[dir ^ 1] = current;
            let parent_bit = self.nodes[parent].bit_index;
            let parent_dir = data.bit(parent_bit) as usize;
            self.nodes[parent].child[parent_dir] = new_node;
            self.nodes[current].parent = new_node;
            self.insert_entry(data, new_node);
        } else if bit_idx > current_bit {
            let new_node = self.new_node(data.clone(), bit_idx, current);
            let dir = data.bit(bit_idx) as usize;
            let current_data_bit = self.nodes[current].data.bit(bit_idx) as usize;
            if current_data_bit == (dir ^ 1) {
                self.nodes[new_node].child[dir ^ 1] = current;
            } else {
                self.nodes[new_node].child[dir ^ 1] = Self::ROOT;
            }
            let cur_dir = data.bit(current_bit) as usize;
            self.nodes[current].child[cur_dir] = new_node;
            self.insert_entry(data, new_node);
        } else {
            let mut new_bit_idx = data.first_1bit();
            let dir = data.bit(bit_idx) as usize;
            let existing_child = self.nodes[current].child[dir];
            if existing_child != Self::ROOT {
                new_bit_idx = self.nodes[existing_child].data.bit_mismatch(&data);
            }
            let new_node = self.new_node(data.clone(), new_bit_idx, current);
            let new_dir = data.bit(new_bit_idx) as usize;
            self.nodes[new_node].child[new_dir ^ 1] = existing_child;
            self.nodes[current].child[dir] = new_node;
            self.insert_entry(data, new_node);
        }
    }
}

/// Builds the `_DIC` nodes Toolbox writes for `keys` (root first).
pub fn build_nodes(keys: &[String]) -> Vec<DictNode> {
    let mut tree = Tree::new();
    for key in keys {
        tree.insert(key);
    }
    let mut nodes = Vec::with_capacity(tree.entries.len());
    for (_, node_index) in &tree.entries {
        let node = &tree.nodes[*node_index];
        let reference = if node.bit_index < 0 {
            u32::MAX
        } else {
            node.bit_index as u32
        };
        let left = tree.entry_index(&tree.nodes[node.child[0]].data) as u16;
        let right = tree.entry_index(&tree.nodes[node.child[1]].data) as u16;
        nodes.push(DictNode {
            reference,
            left,
            right,
            key: Some(node.data.name()),
        });
    }
    if let Some(root) = nodes.first_mut() {
        root.key = None;
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuilds_the_two_entry_shape_dictionary() {
        let nodes = build_nodes(&["Mt_Arm_L".into(), "Mt_Blade_Hide".into()]);
        let scalars: Vec<_> = nodes
            .iter()
            .map(|n| (n.reference, n.left, n.right))
            .collect();
        assert_eq!(scalars, vec![(u32::MAX, 2, 0), (2, 0, 1), (0, 1, 2)]);
        assert_eq!(nodes[1].key.as_deref(), Some("Mt_Arm_L"));
    }

    #[test]
    fn single_key_dictionary_points_at_itself() {
        let nodes = build_nodes(&["Weapon_Sword_019".into()]);
        let scalars: Vec<_> = nodes
            .iter()
            .map(|n| (n.reference, n.left, n.right))
            .collect();
        assert_eq!(scalars, vec![(u32::MAX, 1, 0), (0, 0, 1)]);
    }
}
