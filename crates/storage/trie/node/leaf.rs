use crate::{
    error::StoreError,
    trie::{
        db::TrieDB,
        hashing::{NodeHash, NodeHashRef, NodeHasher, PathKind},
        nibble::NibbleSlice,
        node::BranchNode,
        PathRLP, ValueRLP,
    },
};

use super::{ExtensionNode, Node};

#[derive(Debug, Clone, Default)]
pub struct LeafNode {
    pub hash: NodeHash,
    pub path: PathRLP,
    pub value: ValueRLP,
}

impl LeafNode {
    pub fn new(path: PathRLP, value: ValueRLP) -> Self {
        Self {
            hash: Default::default(),
            path,
            value,
        }
    }

    pub fn get(&self, path: NibbleSlice) -> Result<Option<ValueRLP>, StoreError> {
        if path.cmp_rest(&self.path) {
            Ok(Some(self.value.clone()))
        } else {
            Ok(None)
        }
    }

    pub fn insert(
        mut self,
        db: &mut TrieDB,
        path: NibbleSlice,
        value: ValueRLP,
    ) -> Result<Node, StoreError> {
        // Possible flow paths:
        //   leaf { path => value } -> leaf { path => value }
        //   leaf { path => value } -> branch { 0 => leaf { path => value }, 1 => leaf { path => value } }
        //   leaf { path => value } -> extension { [0], branch { 0 => leaf { path => value }, 1 => leaf { path => value } } }
        //   leaf { path => value } -> extension { [0], branch { 0 => leaf { path => value } } with_value leaf { path => value } }
        //   leaf { path => value } -> extension { [0], branch { 0 => leaf { path => value } } with_value leaf { path => value } } // leafs swapped
        self.hash.mark_as_dirty();
        if path.cmp_rest(&self.path) {
            self.value = value;
            Ok(self.clone().into())
        } else {
            let offset = path.clone().count_prefix_slice(&{
                let mut value_path = NibbleSlice::new(&self.path);
                value_path.offset_add(path.offset());
                value_path
            });

            let mut path_branch = path.clone();
            path_branch.offset_add(offset);

            let absolute_offset = path_branch.offset();
            let branch_node = if absolute_offset == 2 * path.as_ref().len() {
                let mut choices = [Default::default(); 16];
                // TODO: Dedicated method.
                choices[NibbleSlice::new(self.path.as_ref())
                    .nth(absolute_offset)
                    .unwrap() as usize] = db.insert_node(self.clone().into())?;

                BranchNode::new_with_value(choices, path.data(), value.clone())
            } else if absolute_offset == 2 * self.path.len() {
                let new_leaf = LeafNode::new(path.data(), value.clone());

                let child_ref = db.insert_node(new_leaf.into())?;
                BranchNode::new_with_value(
                    {
                        let mut choices = [Default::default(); 16];
                        choices[path_branch.next().unwrap() as usize] = child_ref;
                        choices
                    },
                    self.path.clone(),
                    self.value,
                )
            } else {
                let new_leaf = LeafNode::new(path.data(), value.clone());

                let child_ref = db.insert_node(new_leaf.into())?;
                BranchNode::new({
                    let mut choices = [Default::default(); 16];
                    choices[NibbleSlice::new(self.path.as_ref())
                        .nth(absolute_offset)
                        .unwrap() as usize] = db.insert_node(self.clone().into())?;
                    choices[path_branch.next().unwrap() as usize] = child_ref;
                    choices
                })
            };

            let final_node = if offset != 0 {
                let branch_ref = db.insert_node(Node::Branch(branch_node))?;
                ExtensionNode::new(path.split_to_vec(offset), branch_ref).into()
            } else {
                branch_node.into()
            };

            Ok(final_node)
        }
    }

    pub fn remove(self, path: NibbleSlice) -> Result<(Option<Node>, Option<ValueRLP>), StoreError> {
        Ok(if path.cmp_rest(&self.path) {
            (None, Some(self.value))
        } else {
            (Some(self.into()), None)
        })
    }
    pub fn compute_hash(&self, path_offset: usize) -> Result<NodeHashRef, StoreError> {
        if let Some(hash) = self.hash.extract_ref() {
            return Ok(hash);
        }
        let encoded_value = &self.value;
        let encoded_path = &self.path;

        let mut path_slice = NibbleSlice::new(encoded_path);
        path_slice.offset_add(path_offset);

        Ok(compute_leaf_hash(
            &self.hash,
            path_slice,
            encoded_value.as_ref(),
        ))
    }
}

pub fn compute_leaf_hash<'a>(
    hash: &'a NodeHash,
    path: NibbleSlice,
    value: &[u8],
) -> NodeHashRef<'a> {
    let path_len = NodeHasher::path_len(path.len());
    let value_len = NodeHasher::bytes_len(value.len(), value.first().copied().unwrap_or_default());

    let mut hasher = NodeHasher::new(hash);
    hasher.write_list_header(path_len + value_len);
    hasher.write_path_slice(&path, PathKind::Leaf);
    hasher.write_bytes(value);
    hasher.finalize()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pmt_node;
    use crate::trie::node_ref::NodeRef;
    use crate::trie::Trie;

    #[test]
    fn new() {
        let node = LeafNode::new(Default::default(), Default::default());
        assert_eq!(node.path, PathRLP::default());
        assert_eq!(node.value, PathRLP::default());
    }

    #[test]
    fn get_some() {
        let node = pmt_node! { @(trie)
            leaf { vec![0x12] => vec![0x12, 0x34, 0x56, 0x78] }
        };

        assert_eq!(
            node.get(NibbleSlice::new(&[0x12])).unwrap(),
            Some(vec![0x12, 0x34, 0x56, 0x78]),
        );
    }

    #[test]
    fn get_none() {
        let node = pmt_node! { @(trie)
            leaf { vec![0x12] => vec![0x12, 0x34, 0x56, 0x78] }
        };

        assert!(node.get(NibbleSlice::new(&[0x34])).unwrap().is_none());
    }

    #[test]
    fn insert_replace() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            leaf { vec![0x12] => vec![0x12, 0x34, 0x56, 0x78] }
        };

        let node = node
            .insert(&mut trie.db, NibbleSlice::new(&[0x12]), vec![0x13])
            .unwrap();
        let node = match node {
            Node::Leaf(x) => x,
            _ => panic!("expected a leaf node"),
        };

        assert_eq!(node.path, vec![0x12]);
        assert_eq!(node.value, vec![0x13]);
        assert!(node.hash.extract_ref().is_none());
    }

    #[test]
    fn insert_branch() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            leaf { vec![0x12] => vec![0x12, 0x34, 0x56, 0x78] }
        };
        let path = NibbleSlice::new(&[0x22]);
        let value = vec![0x23];
        let node = node
            .insert(&mut trie.db, path.clone(), value.clone())
            .unwrap();
        let node = match node {
            Node::Branch(x) => x,
            _ => panic!("expected a branch node"),
        };
        // New branch should contain the first node
        assert!(node.choices.iter().any(|x| x == &NodeRef::new(0)));
        assert_eq!(node.get(&trie.db, path).unwrap(), Some(value));
    }

    #[test]
    fn insert_extension_branch() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            leaf { vec![0x12] => vec![0x12, 0x34, 0x56, 0x78] }
        };

        let path = NibbleSlice::new(&[0x13]);
        let value = vec![0x15];

        let node = node
            .insert(&mut trie.db, path.clone(), value.clone())
            .unwrap();

        assert!(matches!(node, Node::Extension(_)));
        assert_eq!(node.get(&trie.db, path).unwrap(), Some(value));
    }

    #[test]
    fn insert_extension_branch_value_self() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            leaf { vec![0x12] => vec![0x12, 0x34, 0x56, 0x78] }
        };

        let path = NibbleSlice::new(&[0x12, 0x34]);
        let value = vec![0x17];

        let node = node
            .insert(&mut trie.db, path.clone(), value.clone())
            .unwrap();

        assert!(matches!(node, Node::Extension(_)));
        assert_eq!(node.get(&trie.db, path).unwrap(), Some(value));
    }

    #[test]
    fn insert_extension_branch_value_other() {
        let mut trie = Trie::new_temp();
        let node = pmt_node! { @(trie)
            leaf { vec![0x12, 0x34] => vec![0x12, 0x34, 0x56, 0x78] }
        };

        let path = NibbleSlice::new(&[0x12]);
        let value = vec![0x17];

        let node = node
            .insert(&mut trie.db, path.clone(), value.clone())
            .unwrap();

        assert!(matches!(node, Node::Extension(_)));
        assert_eq!(node.get(&trie.db, path).unwrap(), Some(value));
    }

    // An insertion that returns branch [value=(x)] -> leaf (y) is not possible because of the path
    // restrictions: nibbles come in pairs. If the first nibble is different, the node will be a
    // branch but it cannot have a value. If the second nibble is different, then it'll be an
    // extension followed by a branch with value and a child.
    //
    // Because of that, the two tests that would check those cases are neither necessary nor
    // possible.

    #[test]
    fn remove_self() {
        let node = LeafNode::new(vec![0x12, 0x34], vec![0x12, 0x34, 0x56, 0x78]);
        let (node, value) = node.remove(NibbleSlice::new(&[0x12, 0x34])).unwrap();

        assert!(node.is_none());
        assert_eq!(value, Some(vec![0x12, 0x34, 0x56, 0x78]));
    }

    #[test]
    fn remove_none() {
        let node = LeafNode::new(vec![0x12, 0x34], vec![0x12, 0x34, 0x56, 0x78]);

        let (node, value) = node.remove(NibbleSlice::new(&[0x12])).unwrap();

        assert!(node.is_some());
        assert_eq!(value, None);
    }

    #[test]
    fn compute_hash() {
        let node = LeafNode::new(b"key".to_vec(), b"value".to_vec());

        let node_hash_ref = node.compute_hash(0).unwrap();
        assert_eq!(
            node_hash_ref.as_ref(),
            &[0xCB, 0x84, 0x20, 0x6B, 0x65, 0x79, 0x85, 0x76, 0x61, 0x6C, 0x75, 0x65],
        );
    }

    #[test]
    fn compute_hash_long() {
        let node = LeafNode::new(b"key".to_vec(), b"a comparatively long value".to_vec());

        let node_hash_ref = node.compute_hash(0).unwrap();
        assert_eq!(
            node_hash_ref.as_ref(),
            &[
                0xEB, 0x92, 0x75, 0xB3, 0xAE, 0x09, 0x3A, 0x17, 0x75, 0x7C, 0xFB, 0x42, 0xF7, 0xD5,
                0x57, 0xF9, 0xE5, 0x77, 0xBD, 0x5B, 0xEB, 0x86, 0xA8, 0x68, 0x49, 0x91, 0xA6, 0x5B,
                0x87, 0x5F, 0x80, 0x7A,
            ],
        );
    }
}