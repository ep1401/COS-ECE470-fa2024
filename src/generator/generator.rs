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
    // let interval = time::Duration::from_millis(10 * theta);
    let interval = time::Duration::from_millis((4.8_f64 * theta as f64) as u64);


    loop {
        // Get the current tip of the blockchain
        let tip;
        {
            let blockchain = self.blockchain.lock().expect("Failed to lock blockchain");
            tip = blockchain.tip().clone();
        }

        // Choose the receiver address
        let receiver = self.receiver_addresses[receiver_index];

        // For simplicity, assume the sender has sufficient balance
        let mut rng = rand::thread_rng();
        let value = rng.gen_range(1..=100); // Random transaction value between 1 and 100
        let nonce = rng.gen_range(1..=1000); // Random nonce for testing

        // Create a new transaction
        let tx = Transaction {
            sender: self.address,
            receiver,
            value,
            account_nonce: nonce,
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
        println!("TransactionGenerator - Broadcast transaction: {:?}", tx_hash);

        // Alternate receiver address
        receiver_index = 1 - receiver_index;

        // Control the rate of transaction generation
        thread::sleep(interval);
    }
}
}