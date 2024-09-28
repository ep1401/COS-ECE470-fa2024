use crossbeam::channel::Receiver;
use log::{debug, info};
use crate::types::block::Block;
use crate::network::server::Handle as ServerHandle;
use std::sync::{Arc, Mutex};
use std::thread;
use crate::blockchain::Blockchain;
use crate::types::hash::Hashable;

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
            let _block = self.finished_block_chan.recv().expect("Receive finished block error");
            // TODO for student: insert this finished block to blockchain, and broadcast this block hash
            // Insert the block into the blockchain
            let mut blockchain_ = self.blockchain.lock().unwrap();
            blockchain_.insert(&_block);

            // TODO: Broadcast the block hash to the network (not required in this part)
            // self.server.broadcast(block.hash());  // Placeholder for future network broadcasting
        }
    }
}
