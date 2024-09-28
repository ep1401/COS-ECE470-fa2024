use crate::types::block::{Block, Header, Content};
use crate::types::hash::H256;
use crate::types::hash::Hashable;
use std::collections::HashMap;

pub struct Blockchain {
    pub blocks: HashMap<H256, Block>,
    pub tip: H256,  // The hash of the block at the tip of the longest chain
}

impl Blockchain {
    /// Create a new blockchain, only containing the genesis block
    pub fn new() -> Self {
        // Set fixed values for the genesis block header
        let genesis_header = Header {
            parent: H256::from([0x00; 32]),  // No parent for the genesis block, so all zeros
            nonce: 0,                        // Set nonce to 0 for the genesis block
            difficulty: [255u8; 32].into(),  // Highest difficulty
            timestamp: 0,                    // A fixed timestamp for the genesis block
            merkle_root: H256::from([0x00; 32]), // Example merkle root for no transactions
        };

        // Genesis block has no transactions (empty content)
        let genesis_content = Content {
            transactions: vec![],  // Empty content for genesis block
        };

        // Create the genesis block with the fixed header and empty content
        let genesis_block = Block {
            header: genesis_header,
            content: genesis_content,
        };

        // Hash the genesis block
        let genesis_hash = genesis_block.hash();

        // Initialize the blockchain with the genesis block
        let mut blocks = HashMap::new();
        blocks.insert(genesis_hash, genesis_block);

        Self {
            blocks,
            tip: genesis_hash,  // The tip is the genesis block initially
        }
    }

    /// Insert a block into blockchain
    pub fn insert(&mut self, block: &Block) {
        let block_hash = block.hash();
        self.blocks.insert(block_hash, block.clone());

        // For simplicity, assume this block always extends the longest chain
        self.tip = block_hash;
    }

    /// Get the last block's hash of the longest chain
    pub fn tip(&self) -> H256 {
        self.tip
    }

    /// Get all blocks' hashes of the longest chain, ordered from genesis to the tip
    pub fn all_blocks_in_longest_chain(&self) -> Vec<H256> {
        // unimplemented!()
        let mut chain = Vec::new();
        let mut current_hash = self.tip;

        // Traverse backwards from the tip to the genesis
        while let Some(block) = self.blocks.get(&current_hash) {
            chain.push(current_hash);
            current_hash = block.get_parent();
        }

        chain.reverse();  // Reverse to get genesis to tip order
        chain
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::block::generate_random_block;
    use crate::types::hash::Hashable;

    #[test]
    fn insert_one() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();
        let block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);
        assert_eq!(blockchain.tip(), block.hash());

    }

    /*
    #[test]
    fn insert_multiple_blocks() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();

        // Insert 50 blocks sequentially
        let mut previous_hash = genesis_hash;
        for _ in 0..50 {
            let block = generate_random_block(&previous_hash);
            blockchain.insert(&block);
            previous_hash = block.hash();
        }

        // Check if the tip is correct (last inserted block)
        assert_eq!(blockchain.tip(), previous_hash);

        // Verify that the longest chain has 51 blocks (genesis + 50 blocks)
        let chain = blockchain.all_blocks_in_longest_chain();
        assert_eq!(chain.len(), 51);
    }
    

    #[test]
    fn branching_scenario() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();

        // Insert 5 blocks sequentially on the main chain
        let mut previous_hash = genesis_hash;
        for _ in 0..5 {
            let block = generate_random_block(&previous_hash);
            blockchain.insert(&block);
            previous_hash = block.hash();
        }

        // Create a fork: Add 3 blocks to a block earlier in the chain
        let fork_block_hash = blockchain.all_blocks_in_longest_chain()[3]; // Fork at the 3rd block
        let mut fork_hash = fork_block_hash;
        for _ in 0..3 {
            let fork_block = generate_random_block(&fork_hash);
            blockchain.insert(&fork_block);
            fork_hash = fork_block.hash();
        }

        // Add 2 more blocks to the main chain (making it longer than the fork)
        for _ in 0..2 {
            let block = generate_random_block(&previous_hash);
            blockchain.insert(&block);
            previous_hash = block.hash();
        }

        // The tip should point to the main chain, as it is now the longest chain
        assert_eq!(blockchain.tip(), previous_hash);

        // Check that the longest chain has 8 blocks (5 from the main chain + 2 additional)
        let chain = blockchain.all_blocks_in_longest_chain();
        assert_eq!(chain.len(), 8);
    }

    #[test]
    fn verify_longest_chain_order() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();

        // Insert 10 blocks sequentially
        let mut previous_hash = genesis_hash;
        let mut inserted_hashes = vec![genesis_hash];
        for _ in 0..10 {
            let block = generate_random_block(&previous_hash);
            blockchain.insert(&block);
            previous_hash = block.hash();
            inserted_hashes.push(previous_hash);
        }

        // Verify that all blocks' hashes in the longest chain match the inserted order
        let chain = blockchain.all_blocks_in_longest_chain();
        assert_eq!(chain, inserted_hashes);
    }

    #[test]
    fn fork_with_equal_length() {
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();

        // Insert 5 blocks sequentially on the main chain
        let mut previous_hash = genesis_hash;
        for _ in 0..5 {
            let block = generate_random_block(&previous_hash);
            blockchain.insert(&block);
            previous_hash = block.hash();
        }

        // Create a fork from the 3rd block and add 2 blocks to the fork
        let fork_block_hash = blockchain.all_blocks_in_longest_chain()[3]; // Fork at the 3rd block
        let mut fork_hash = fork_block_hash;
        for _ in 0..2 {
            let fork_block = generate_random_block(&fork_hash);
            blockchain.insert(&fork_block);
            fork_hash = fork_block.hash();
        }

        // Both chains have the same length (5 blocks). The blockchain should still function.
        assert!(blockchain.tip() == previous_hash || blockchain.tip() == fork_hash);

        // The longest chain should have 6 blocks (including the genesis block)
        let chain = blockchain.all_blocks_in_longest_chain();
        assert_eq!(chain.len(), 6);
    }
    */

}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST