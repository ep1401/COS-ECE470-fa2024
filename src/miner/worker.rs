use crossbeam::channel::Receiver;
use log::{debug, info};
use crate::types::block::Block;
use crate::network::server::Handle as ServerHandle;
use std::sync::{Arc, Mutex};
use std::thread;
use crate::blockchain::Blockchain;
use crate::types::hash::Hashable;
use crate::types::hash::H256;
use crate::network::message::Message;

#[derive(Clone)]
pub struct Worker {
    server: ServerHandle,
    finished_block_chan: Receiver<Block>,
    blockchain: Arc<Mutex<Blockchain>>,
}

impl Worker {
    pub fn new(
        server: &ServerHandle,
        finished_block_chan: Receiver<Block>,
        blockchain: Arc<Mutex<Blockchain>>,  // Add blockchain to the arguments
    ) -> Self {
        Self {
            server: server.clone(),
            finished_block_chan,
            blockchain: Arc::clone(&blockchain),  // Clone the Arc for thread-safe access
        }
    }

    pub fn start(self) {
        thread::Builder::new()
            .name("miner-worker".to_string())
            .spawn(move || {
                self.worker_loop();
            })
            .unwrap();
        info!("Miner initialized into paused mode");
    }

    fn worker_loop(&self) {
        loop {
            // Receive the mined block
            let block = self.finished_block_chan.recv().expect("Receive finished block error");
            let block_hash = block.hash();
            let parent_hash = block.get_parent();
    
            // Lock the blockchain for thread-safe access
            let mut blockchain = self.blockchain.lock().unwrap();
    
            // Check if the block's parent is still the tip
            if blockchain.tip() != parent_hash {
                println!("Skipping block {} because the tip has changed.", block_hash);
                continue; // Skip insertion if the tip has already moved forward
            }
    
            // Check if the block already exists in the blockchain
            if blockchain.blocks.contains_key(&block_hash) {
                println!("Block already exists: {}", block_hash);
                continue; // Skip inserting if the block is already present
            }
    
            // Insert the block into the blockchain
            blockchain.insert(&block);
            info!("Block inserted: {}", block_hash);
    
            // Notify all miners to update their tip
            self.server.broadcast(Message::NewBlockHashes(vec![block_hash]));
            self.server.update();
        }
    }
    
    
}
