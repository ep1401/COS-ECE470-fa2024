use super::hash::{Hashable, H256};
use sha2::{Sha256, Digest};
use std::convert::TryFrom; // for helper function

/// A Merkle tree.
#[derive(Debug, Default)]
pub struct MerkleTree {
    leaves: Vec<H256>,     // Hashes of the leaves
    root: H256,            // Root of the Merkle tree
    tree: Vec<Vec<H256>>,  // Layers of the tree
}

impl MerkleTree {
    pub fn new<T>(data: &[T]) -> Self
    where
        T: Hashable,
    {
        // If the input data is empty, return an empty Merkle tree with a default root.
        if data.is_empty() {
            return MerkleTree {
                leaves: vec![],
                root: H256::default(),
                tree: vec![],
            };
        }

        let mut leaves: Vec<H256> = data.iter().map(|datum| datum.hash()).collect();

        // If there is only one leaf, the root is that leaf's hash.
        if leaves.len() == 1 {
            return MerkleTree {
                leaves: leaves.clone(),
                root: leaves[0],
                tree: vec![leaves.clone()],
            };
        }

        // Initialize the tree with the leaves
        let mut tree = vec![];
        tree.push(leaves.clone());

        // Build the tree layer by layer by hashing pairs of nodes.
        while leaves.len() > 1 {
            // Pad the layer if it has an odd number of nodes
            if leaves.len() % 2 != 0 {
                leaves.push(leaves[leaves.len() - 1].clone());
            }

            let mut next_layer = vec![];
            for chunk in leaves.chunks(2) {
                // Ensure that the chunk always has two elements
                let parent_hash = hash_two(&chunk[0], &chunk[1]);
                next_layer.push(parent_hash);
            }

            tree.push(next_layer.clone());
            leaves = next_layer;
        }

        let root = leaves[0]; // The root is the only remaining node
        MerkleTree {
            leaves: tree[0].clone(),
            root,
            tree,
        }
    }
    pub fn root(&self) -> H256 {
        self.root
    }

    /// Returns the Merkle Proof of data at index i
    pub fn proof(&self, index: usize) -> Vec<H256> {
        if index >= self.leaves.len() {
            return vec![]; // Return empty vector for invalid index
        }
        let mut proof = vec![];
        let mut idx = index;
    
        // Traverse up the tree, collecting sibling hashes for the proof.
        for layer in &self.tree[..self.tree.len() - 1] {
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
    
            // Check if the sibling index is within bounds
            if sibling_idx < layer.len() {
                proof.push(layer[sibling_idx]);
            } else {
                // If the index is out of bounds, use the current node's hash (due to padding)
                proof.push(layer[idx]);
            }
            idx /= 2;
        }
        proof
    }
    
}

/// Verify that the datum hash with a vector of proofs will produce the Merkle root. Also need the
/// index of datum and `leaf_size`, the total number of leaves.
pub fn verify(root: &H256, datum: &H256, proof: &[H256], index: usize, leaf_size: usize) -> bool {
    if index >= leaf_size {
        return false; // Return false for invalid index
    }

    let mut current_hash = *datum;
    let mut idx = index;

    // Combine the datum with each hash in the proof to compute the root
    for sibling in proof {
        if idx % 2 == 0 {
            current_hash = hash_two(&current_hash, sibling);
        } else {
            current_hash = hash_two(sibling, &current_hash);
        }
        idx /= 2;
    }

    // Check if the computed root matches the expected root
    current_hash == *root
}

// Helper function to compute hash of two H236 values concatenated 
fn hash_two(a: &H256, b: &H256) -> H256 {
    let mut hasher = Sha256::new();
    hasher.update(a.as_ref());
    hasher.update(b.as_ref());
    let result = hasher.finalize(); // This returns a GenericArray<u8, 32>
    
    // Explicit conversion to [u8; 32]
    H256::from(<[u8; 32]>::try_from(result.as_slice()).expect("Hash output should be 32 bytes"))
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use crate::types::hash::H256;
    use super::*;

    macro_rules! gen_merkle_tree_data {
        () => {{
            vec![
                (hex!("0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d")).into(),
                (hex!("0101010101010101010101010101010101010101010101010101010101010202")).into(),
            ]
        }};
    }

    #[test]
    fn merkle_root() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let root = merkle_tree.root();
        assert_eq!(
            root,
            (hex!("6b787718210e0b3b608814e04e61fde06d0df794319a12162f287412df3ec920")).into()
        );
    }

    #[test]
    fn merkle_proof() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let proof = merkle_tree.proof(0);
        assert_eq!(proof,
                   vec![hex!("965b093a75a75895a351786dd7a188515173f6928a8af8c9baa4dcff268a4f0f").into()]
        );
    }

    #[test]
    fn merkle_verifying() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let proof = merkle_tree.proof(0);
        assert!(verify(&merkle_tree.root(), &input_data[0].hash(), &proof, 0, input_data.len()));
    }

}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST