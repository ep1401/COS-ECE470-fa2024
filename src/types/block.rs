use serde::{Serialize, Deserialize};
use crate::types::hash::{H256, Hashable};
use crate::types::transaction::SignedTransaction;  // Assuming SignedTransaction is defined in transaction module
use std::time::{SystemTime, UNIX_EPOCH};
use sha2::{Sha256, Digest};

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

impl Hashable for Block {
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
    let difficulty = H256::from([0xff; 32]);  // Placeholder value for difficulty

    // Merkle root for empty content
    let merkle_root = H256::from([0x00; 32]);

    let header = Header {
        parent: *parent,
        nonce,
        difficulty,
        timestamp,
        merkle_root,
    };

    let content = Content {
        transactions: vec![],  // Empty content for now
    };

    Block { header, content }
}