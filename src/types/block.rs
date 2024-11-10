use serde::{Serialize, Deserialize};
use crate::types::hash::{H256, Hashable};
use crate::types::transaction::SignedTransaction;  
use std::time::{SystemTime, UNIX_EPOCH};
use sha2::{Sha256, Digest};
use rand::Rng;
use crate::types::merkle::MerkleTree;

use std::collections::HashMap;
use crate::types::address::Address;

pub struct BlockState {
    //block hash -> block state (account address -> (account nonce, account balance))
    pub block_state_map: HashMap<H256, HashMap<Address, (u32, u32)>>
}

impl BlockState {
    pub fn new() -> Self {
        return BlockState {
            block_state_map: HashMap::<H256, HashMap<Address, (u32, u32)>>::new()
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Header {
    pub parent: H256,
    pub nonce: u32,
    pub difficulty: H256,
    pub timestamp: u128,
    pub merkle_root: H256,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Content {
    pub transactions: Vec<SignedTransaction>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub header: Header,
    pub content: Content,
}

// Implement Hashable for Header
impl Hashable for Header {
    fn hash(&self) -> H256 {
        // Serialize the header and hash it using sha2's Sha256
        let serialized = bincode::serialize(self).unwrap();
        let mut hasher = Sha256::new();  // Initialize Sha256 hasher
        hasher.update(&serialized);  // Feed the serialized data to the hasher
        let result = hasher.finalize();  // Finalize and get the hash result
        let hash: [u8; 32] = result.into();  // Convert the result into an array of bytes
        hash.into()  // Convert the byte array into H256
    }
}

impl Hashable for Block {
    fn hash(&self) -> H256 {
        // Instead of hashing the whole block, we hash the header
        self.header.hash()
    }
}

impl Block {
    pub fn get_parent(&self) -> H256 {
        self.header.parent
    }

    pub fn get_difficulty(&self) -> H256 {
        self.header.difficulty
    }
}

#[cfg(any(test, test_utilities))]
pub fn generate_random_block(parent: &H256) -> Block {
    let nonce: u32 = rand::random();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let mut rng = rand::thread_rng();
    let difficulty = H256::from(rng.gen::<[u8; 32]>());
    let merkle_tree_ = MerkleTree::new(&Vec::<SignedTransaction>::new());

    let header = Header {
        parent: *parent,
        nonce: nonce,
        difficulty: difficulty,
        timestamp: timestamp,
        merkle_root: merkle_tree_.root(),
    };

    let content = Content {
        transactions: Vec::<SignedTransaction>::new(),  
    };

    let new_block = Block {
        header: header,
        content: content
    };

    return new_block;
}