use log::info;
use crossbeam::channel::{unbounded, Receiver, Sender};
use ring::signature::Ed25519KeyPair;
use std::sync::{Arc, Mutex};
use std::time;
use std::thread;
use rand::Rng;


use crate::types::address::Address;
use crate::blockchain::Blockchain;
use crate::types::block::BlockState;
use crate::types::transaction::{SignedTransaction, Transaction, sign};
use crate::network::server::Handle as ServerHandle;
use crate::network::message::Message;
use crate::miner::Mempool;
use crate::types::hash::H256;


use ring::signature::KeyPair;
use crate::types::hash::Hashable;
use std::collections::HashMap;


#[derive(Clone)]
pub struct TransactionGenerator {
   finished_tx_chan: Sender<SignedTransaction>,
   blockchain: Arc<Mutex<Blockchain>>,
   mempool: Arc<Mutex<Mempool>>,
   server: ServerHandle,
   address: Address,
   keypair: Arc<Ed25519KeyPair>, // Wrapped in Arc to make it clonable
   block_state_map: Arc<Mutex<BlockState>>,
   receiver_addresses: [Address; 2],
}


impl TransactionGenerator {
   /// Creates a new TransactionGenerator instance
   pub fn new(
       blockchain: Arc<Mutex<Blockchain>>,
       address: Address,
       keypair: Arc<Ed25519KeyPair>, // Now expects Arc<Ed25519KeyPair>
       block_state_map: Arc<Mutex<BlockState>>,
       receiver_addresses: [Address; 2],
       server: ServerHandle,
       mempool: Arc<Mutex<Mempool>>,
       finished_tx_chan: Sender<SignedTransaction>,
   ) -> Self {
       Self {
           finished_tx_chan,
           blockchain,
           mempool,
           server,
           address,
           keypair,
           block_state_map,
           receiver_addresses,
       }
   }


   pub fn start(self, theta: u64) {
       thread::Builder::new()
           .name("transaction-generator".to_string())
           .spawn(move || {
               self.generate_transactions(theta);
           })
           .unwrap();
       info!("Transaction generator started");
   }


   fn generate_transactions(&self, theta: u64) {
    let mut receiver_index = 0;
    let interval = time::Duration::from_millis((2.5_f64 * theta as f64) as u64);

    loop {
        // Get the current tip of the blockchain
        let tip;
        {
            let blockchain = self.blockchain.lock().expect("Failed to lock blockchain");
            tip = blockchain.tip().clone();
        }

        // Retrieve the state for the sender from the block_state_map
        let mut sender_state = (0, 0);  // Default sender state (nonce, balance)
        {
            let block_state_map = self.block_state_map.lock().unwrap();
            if let Some(state) = block_state_map.block_state_map.get(&tip) {
                if let Some(state_for_sender) = state.get(&self.address) {
                    sender_state = state_for_sender.clone();
                }
            }
        }

        // Debugging output for sender state
        info!("Sender state for {:?}: nonce = {}, balance = {}", self.address, sender_state.0, sender_state.1);

        // If the sender balance is 0, we skip transaction generation
        if sender_state.1 == 0 {
            info!("Skipping transaction for sender {:?}, balance is 0", self.address);
            continue;
        }

        // Generate the transaction value as half the balance or at least 1
        let mut value = sender_state.1 / 2;
        if value == 0 {
            value = 1;
        }

        // Generate the nonce for the transaction (should be the stored sender nonce)
        let nonce = sender_state.0; // Use the stored nonce for the sender
        let account_nonce = nonce + 1; // The account nonce should always be one higher

        // Validate the nonce (should match the sender's current nonce + 1)
        if account_nonce != sender_state.0 + 1 {
            info!("Skipping invalid transaction for sender {:?}, expected nonce: {}, got: {}", 
                  self.address, sender_state.0 + 1, account_nonce);
            continue;  // Skip transaction generation if invalid nonce
        }

        // Choose the receiver address
        let receiver = self.receiver_addresses[receiver_index];

        // Create a new transaction using the state-derived nonce
        let tx = Transaction {
            sender: self.address,
            receiver,
            value,
            account_nonce: account_nonce, // Increment the nonce for the transaction
        };

        // Sign the transaction
        let signature = sign(&tx, &self.keypair);
        let signed_tx = SignedTransaction {
            transaction: tx,
            signature: signature.as_ref().to_vec(),
            public_key: self.keypair.public_key().as_ref().to_vec(),
        };

        // Insert the transaction into the mempool
        {
            let mut mempool = self.mempool.lock().expect("Failed to lock mempool");
            mempool.insert(&signed_tx);
        }

        // Broadcast the transaction
        let tx_hash = signed_tx.hash();
        self.server.broadcast(Message::NewTransactionHashes(vec![tx_hash]));

        // Alternate receiver address
        receiver_index = 1 - receiver_index;

        // Control the rate of transaction generation
        thread::sleep(interval);
    }
}

}