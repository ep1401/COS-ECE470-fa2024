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
  
       loop {
           // Try to acquire lock on the blockchain without blocking
           let tip = match self.blockchain.lock() {
               Ok(blockchain) => blockchain.tip().clone(),
               Err(_) => {
                   log::error!("Failed to lock blockchain, retrying...");
                   continue;
               }
           };
  
           // Try to acquire lock on the block state map
           let tip_state = match self.block_state_map.lock() {
               Ok(state_map) => state_map.block_state_map.get(&tip).cloned(),
               Err(_) => {
                   log::error!("Failed to lock block state map, retrying...");
                   continue;
               }
           };
  
           if tip_state.is_none() {
               log::warn!("Tip state not found, retrying...");
               continue;
           }
           let tip_state = tip_state.unwrap();
  
           let receiver = self.receiver_addresses[receiver_index];
           let sender_balance = tip_state.get(&self.address).unwrap_or(&(0, 0));
  
           if sender_balance.1 == 0 {
               log::info!("Sender balance is zero, skipping transaction generation");
               continue;
           }
  
           let mut rng = rand::thread_rng();
           let value = rng.gen_range(1..=(sender_balance.1 / 2).max(1));
           let nonce = sender_balance.0 + 1;
  
           let tx = Transaction {
               sender: self.address,
               receiver,
               value,
               account_nonce: nonce,
           };
  
           let signature = sign(&tx, &self.keypair);
           let signed_tx = SignedTransaction {
               transaction: tx,
               signature: signature.as_ref().to_vec(),
               public_key: self.keypair.public_key().as_ref().to_vec(),
           };
  
           // Try to acquire lock on mempool without blocking
           {
               let mut mempool = match self.mempool.lock() {
                   Ok(mempool) => mempool,
                   Err(_) => {
                       log::error!("Failed to lock mempool, retrying...");
                       continue;
                   }
               };
               mempool.insert(&signed_tx);
           }
  
           // Broadcast the transaction
           let tx_hash = signed_tx.hash();
           self.server.broadcast(Message::NewTransactionHashes(vec![tx_hash]));
  
           println!("Generated and broadcasted transaction: {:?}", tx_hash);
  
           // Alternate between receiver addresses
           receiver_index = 1 - receiver_index;
  
           // Sleep to control the rate of transaction generation
           if theta != 0 {
               let interval = time::Duration::from_millis(10 * theta);
               thread::sleep(interval);
           }
       }
   }
  
}



