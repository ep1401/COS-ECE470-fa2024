use super::message::Message;
use super::peer;
use super::server::Handle as ServerHandle;
use crate::types::hash::{H256, Hashable};
use crate::types::block::Block;
use crate::blockchain::Blockchain;
use log::{debug, warn, error, info};

use std::sync::{Arc, Mutex}; // For thread-safe access to blockchain
use std::thread; // For spawning worker threads

#[cfg(any(test,test_utilities))]
use super::peer::TestReceiver as PeerTestReceiver;
#[cfg(any(test,test_utilities))]
use super::server::TestReceiver as ServerTestReceiver;
#[derive(Clone)]
pub struct Worker {
    msg_chan: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
    num_worker: usize,
    server: ServerHandle,
    blockchain: Arc<Mutex<Blockchain>>, // Add thread-safe blockchain field
}


impl Worker {
    pub fn new(
        num_worker: usize,
        msg_src: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
        server: &ServerHandle,
        blockchain: Arc<Mutex<Blockchain>>, // Accept blockchain as parameter
    ) -> Self {
        Self {
            msg_chan: msg_src,
            num_worker,
            server: server.clone(),
            blockchain, // Store blockchain in Worker struct
        }
    }

    pub fn start(self) {
        let num_worker = self.num_worker;
        for i in 0..num_worker {
            let cloned = self.clone();
            thread::spawn(move || {
                cloned.worker_loop();
                warn!("Worker thread {} exited", i);
            });
        }
    }

    fn worker_loop(&self) {
        loop {
            let result = smol::block_on(self.msg_chan.recv());
            if let Err(e) = result {
                error!("network worker terminated {}", e);
                break;
            }
            let (msg, mut peer) = result.unwrap();
            let msg: Message = bincode::deserialize(&msg).unwrap();

            match msg {
                Message::Ping(nonce) => {
                    debug!("Ping: {}", nonce);
                    peer.write(Message::Pong(nonce.to_string()));
                }
                Message::Pong(nonce) => {
                    debug!("Pong: {}", nonce);
                }
                Message::NewBlockHashes(hashes) => {
                    // Handle new block hashes (Gossip protocol)
                    let blockchain = self.blockchain.lock().unwrap(); // Access the blockchain
                    let mut missing_blocks: Vec<H256> = Vec::new();
                    for hash in hashes {
                        if !blockchain.blocks.contains_key(&hash) {  // Check if the block exists in the `blocks` map
                            missing_blocks.push(hash);
                        }
                    }
                    if !missing_blocks.is_empty() {
                        peer.write(Message::GetBlocks(missing_blocks));
                    }
                }
                Message::GetBlocks(hashes) => {
                    // Handle GetBlocks message
                    let blockchain = self.blockchain.lock().unwrap(); // Access the blockchain
                    let mut blocks_to_send: Vec<Block> = Vec::new();
                    for hash in hashes {
                        if let Some(block) = blockchain.blocks.get(&hash) {  // Access blocks from `blocks` map
                            blocks_to_send.push(block.clone());
                        }
                    }
                    if !blocks_to_send.is_empty() {
                        peer.write(Message::Blocks(blocks_to_send));
                    }
                }
                Message::Blocks(blocks) => {
                    // Handle Blocks message
                    let mut blockchain = self.blockchain.lock().unwrap(); // Access the blockchain
                    let mut new_block_hashes: Vec<H256> = Vec::new();
                    for block in blocks {
                        if !blockchain.blocks.contains_key(&block.hash()) {  // Check if block already exists
                            blockchain.insert(&block);  // Insert new block into the blockchain
                            new_block_hashes.push(block.hash());
                        }
                    }
                    if !new_block_hashes.is_empty() {
                        self.server.broadcast(Message::NewBlockHashes(new_block_hashes));
                    }
                }
                _ => unimplemented!(),
            }
        }
    }
}

#[cfg(any(test,test_utilities))]
struct TestMsgSender {
    s: smol::channel::Sender<(Vec<u8>, peer::Handle)>
}
#[cfg(any(test,test_utilities))]
impl TestMsgSender {
    fn new() -> (TestMsgSender, smol::channel::Receiver<(Vec<u8>, peer::Handle)>) {
        let (s,r) = smol::channel::unbounded();
        (TestMsgSender {s}, r)
    }

    fn send(&self, msg: Message) -> PeerTestReceiver {
        let bytes = bincode::serialize(&msg).unwrap();
        let (handle, r) = peer::Handle::test_handle();
        smol::block_on(self.s.send((bytes, handle))).unwrap();
        r
    }
}
#[cfg(any(test,test_utilities))]
/// returns two structs used by tests, and an ordered vector of hashes of all blocks in the blockchain
fn generate_test_worker_and_start() -> (TestMsgSender, ServerTestReceiver, Vec<H256>) {
    // Create a test server and its receiver
    let (server, server_receiver) = ServerHandle::new_for_test();

    // Create a test message sender and its corresponding receiver
    let (test_msg_sender, msg_chan) = TestMsgSender::new();

    // Initialize blockchain with a thread-safe wrapper
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));

    // Obtain the vector of block hashes from the genesis to the tip
    let block_hashes = {
        let blockchain_guard = blockchain.lock().unwrap();
        blockchain_guard.all_blocks_in_longest_chain() // Use the all_blocks_in_longest_chain function
    };

    // Initialize the worker with the blockchain and other parameters
    let worker = Worker::new(1, msg_chan, &server, Arc::clone(&blockchain));
    worker.start();

    // Return the message sender, server receiver, and the vector of block hashes
    (test_msg_sender, server_receiver, block_hashes)
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod test {
    use ntest::timeout;
    use crate::types::block::generate_random_block;
    use crate::types::hash::{Hashable, H256};
    
    use super::super::message::Message;
    use super::generate_test_worker_and_start;

    #[test]
    #[timeout(60000)]
    fn reply_new_block_hashes() {
        let (test_msg_sender, _server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut peer_receiver = test_msg_sender.send(Message::NewBlockHashes(vec![random_block.hash()]));
        let reply = peer_receiver.recv();
        if let Message::GetBlocks(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }
    }
    #[test]
    #[timeout(60000)]
    fn reply_get_blocks() {
        let (test_msg_sender, _server_receiver, v) = generate_test_worker_and_start();
        let h = v.last().unwrap().clone();
        let mut peer_receiver = test_msg_sender.send(Message::GetBlocks(vec![h.clone()]));
        let reply = peer_receiver.recv();
        if let Message::Blocks(v) = reply {
            assert_eq!(1, v.len());
            assert_eq!(h, v[0].hash())
        } else {
            panic!();
        }
    }
    #[test]
    #[timeout(60000)]
    fn reply_blocks() {
        let (test_msg_sender, server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut _peer_receiver = test_msg_sender.send(Message::Blocks(vec![random_block.clone()]));
        let reply = server_receiver.recv().unwrap();
        if let Message::NewBlockHashes(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST