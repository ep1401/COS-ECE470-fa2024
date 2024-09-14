use crate::types::address::Address;
use serde::{Serialize, Deserialize};
use ring::signature::{Ed25519KeyPair, Signature, UnparsedPublicKey, KeyPair, ED25519};
use rand::Rng;
use bincode;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Transaction {
    pub sender: Address,
    pub receiver: Address,
    pub value: u64,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl SignedTransaction {
    /// Creates a SignedTransaction from a transaction and a key pair
    pub fn new(transaction: Transaction, signature: &Signature, key_pair: &Ed25519KeyPair) -> Self {
        // Convert signature to a vector of bytes
        let signature_vector: Vec<u8> = signature.as_ref().to_vec();
        
        // Convert the public key to a vector of bytes
        let public_key_vector: Vec<u8> = key_pair.public_key().as_ref().to_vec();

        SignedTransaction {
            transaction,
            signature: signature_vector,
            public_key: public_key_vector,
        }
    }
}

/// Create digital signature of a transaction
pub fn sign(t: &Transaction, key: &Ed25519KeyPair) -> Signature {
    let serialized_transaction = bincode::serialize(t).expect("Failed to serialize transaction");
    key.sign(&serialized_transaction)
}

/// Verify digital signature of a transaction, using public key instead of secret key
pub fn verify(t: &Transaction, public_key: &[u8], signature: &[u8]) -> bool {
    let serialized_transaction = bincode::serialize(t).expect("Failed to serialize transaction");
    let public_key = UnparsedPublicKey::new(&ED25519, public_key);
    public_key.verify(&serialized_transaction, signature).is_ok()
}

#[cfg(any(test, test_utilities))]
pub fn generate_random_transaction() -> Transaction {
    let mut rng = rand::thread_rng();
    
    // Generate random public key bytes (32 bytes)
    let random_bytes_sender: [u8; 32] = rng.gen();
    let random_bytes_receiver: [u8; 32] = rng.gen();

    // Generate sender and receiver addresses from random bytes
    let sender = Address::from_public_key_bytes(&random_bytes_sender);
    let receiver = Address::from_public_key_bytes(&random_bytes_receiver);

    // Generate a random value for the transaction
    let value = rng.gen_range(1..1000);

    Transaction {
        sender,
        receiver,
        value,
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::key_pair;
    use ring::signature::KeyPair;


    #[test]
    fn sign_verify() {
        let t = generate_random_transaction();
        let key = key_pair::random();
        let signature = sign(&t, &key);
        assert!(verify(&t, key.public_key().as_ref(), signature.as_ref()));
    }
    #[test]
    fn sign_verify_two() {
        let t = generate_random_transaction();
        let key = key_pair::random();
        let signature = sign(&t, &key);
        let key_2 = key_pair::random();
        let t_2 = generate_random_transaction();
        assert!(!verify(&t_2, key.public_key().as_ref(), signature.as_ref()));
        assert!(!verify(&t, key_2.public_key().as_ref(), signature.as_ref()));
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST