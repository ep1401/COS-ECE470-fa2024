use super::message::Message;
use super::peer;
use super::server::Handle as ServerHandle;
use crate::miner::Mempool;
use crate::types::block::{Block, BlockState};
use crate::types::hash::{H256, Hashable};
use crate::types::transaction::{SignedTransaction, verify};
use std::sync::{Arc, Mutex};
use crate::blockchain::{Blockchain, DIFFICULTY};

use log::{debug, warn, error};

use std::thread;

#[cfg(any(test,test_utilities))]
use super::peer::TestReceiver as PeerTestReceiver;
#[cfg(any(test,test_utilities))]
use super::server::TestReceiver as ServerTestReceiver;
#[derive(Clone)]
pub struct Worker {
    msg_chan: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
    num_worker: usize,
    server: ServerHandle,
    blockchain: Arc<Mutex<Blockchain>>,
    mempool: Arc<Mutex<Mempool>>,
    block_state_map: Arc<Mutex<BlockState>>
}

pub struct OrphanBuffer {
    pub orphans: Vec<Block>
}

impl OrphanBuffer {
    pub fn new() -> Self {
        return Self {
            orphans: Vec::<Block>::new()
        }
    }
}

impl Worker {
    pub fn new(
        num_worker: usize,
        msg_src: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
        server: &ServerHandle,
        blockchain: &Arc<Mutex<Blockchain>>,
        mempool: &Arc<Mutex<Mempool>>,
        block_state_map: &Arc<Mutex<BlockState>>
    ) -> Self {
        Self {
            msg_chan: msg_src,
            num_worker,
            server: server.clone(),
            blockchain: Arc::clone(blockchain),
            mempool: Arc::clone(mempool),
            block_state_map: Arc::clone(block_state_map)
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
            let msg = result.unwrap();
            let (msg, mut peer) = msg;
            let msg: Message = bincode::deserialize(&msg).unwrap();
            match msg {
                Message::Ping(nonce) => {
                    debug!("Ping: {}", nonce);
                    peer.write(Message::Pong(nonce.to_string()));
                }
                Message::Pong(nonce) => {
                    debug!("Pong: {}", nonce);
                }
                Message::NewBlockHashes(block_hashes) => {
                    let mut missing_blocks: Vec<H256> = Vec::<H256>::new();
                    let block_map = self.blockchain.lock().unwrap().blocks.clone(); 
                    for block in block_hashes {
                        if !block_map.contains_key(&block) {
                            missing_blocks.push(block);
                        }
                    }
                    //https://piazza.com/class/kykjhx727ab1ge?cid=84
                    if missing_blocks.len() != 0 {
                        peer.write(Message::GetBlocks(missing_blocks));
                    }
                }
                Message::NewTransactionHashes(tx_hashes) => {
                    let mut missing_txs: Vec<H256> = Vec::<H256>::new();
                    let tx_set = self.mempool.lock().unwrap().transaction_set.clone();
                    for tx in tx_hashes {
                        if !tx_set.contains(&tx) {
                            missing_txs.push(tx);
                        }
                    }
                    if missing_txs.len() != 0 {
                        peer.write(Message::GetTransactions(missing_txs));
                    }
                }
                Message::GetBlocks(blocks) => {
                    let mut send_blocks: Vec<Block> = Vec::<Block>::new();
                    let block_map = self.blockchain.lock().unwrap().blocks.clone(); 
                    for block in blocks {
                        if block_map.contains_key(&block) {
                            let result: &Block = block_map.get(&block).unwrap();
                            send_blocks.push(result.clone());
                        }
                    }
                    //https://piazza.com/class/kykjhx727ab1ge?cid=84
                    if send_blocks.len() != 0 {
                        peer.write(Message::Blocks(send_blocks));
                    }
                }
                Message::GetTransactions(transactions) => {
                    let mut send_transactions: Vec<SignedTransaction> = Vec::<SignedTransaction>::new();
                    let tx_map = self.mempool.lock().unwrap().transaction_map.clone();
                    for transaction in transactions {
                        if tx_map.contains_key(&transaction) {
                            let result: &SignedTransaction = tx_map.get(&transaction).unwrap();
                            send_transactions.push(result.clone());
                        }
                    }
                    if send_transactions.len() != 0 {
                        peer.write(Message::Transactions(send_transactions));
                    }
                }
                Message::Blocks(blocks) => {
                    let mut broadcast_blocks: Vec<H256> = Vec::<H256>::new();
                    let mut parent_blocks: Vec<H256> = Vec::<H256>::new();
                    let mut blockchain = self.blockchain.lock().unwrap();
                    //process_blocks represents blocks to process for orphan blocks
                    let mut process_blocks = Vec::<Block>::new();
                    let mut orphan_buffer: OrphanBuffer = OrphanBuffer::new();
                    'block:for block in blocks {
                        if !blockchain.blocks.contains_key(&block.hash()) {
                            //Proof of Work
                            if !(block.hash() <= DIFFICULTY.into()) {
                                continue;
                            }

                            ///////////////Transaction Checks////////////////////////////////////////////////
                            //here only check for signature
                            for transaction in &block.content.transactions {
                                if !verify(&transaction.transaction, &transaction.public_key, &transaction.signature) {
                                    continue 'block;
                                }
                            }
                            //////////////////////////////////////////////////////////////////////////////////
                            
                            //Parent Check/Orphan Block Check
                            let parent_hash = block.get_parent();
                            if blockchain.blocks.contains_key(&parent_hash) {
                                
                                blockchain.insert(&block);
                                let mut mempool = self.mempool.lock().unwrap();
                                for tx in &block.content.transactions.clone() {
                                    mempool.remove(&tx.hash());
                                }
                                broadcast_blocks.push(block.hash());
                                //need to check for orphans
                                process_blocks.push(block.clone());
                            } else {
                                orphan_buffer.orphans.push(block.clone());
                                parent_blocks.push(parent_hash.clone());
                            }

                            //Orphan Buffer Check
                            let mut keep_orphans = Vec::<Block>::new();
                            while !process_blocks.is_empty() {
                                let block = process_blocks.pop().unwrap();
                                for orphan in orphan_buffer.orphans.clone() {
                                    //block is parent, don't keep orphan
                                    if orphan.get_parent() == block.hash() {
                                        
                                        blockchain.insert(&orphan);
                                        let mut mempool = self.mempool.lock().unwrap();
                                        for tx in block.content.transactions.clone() {
                                            mempool.remove(&tx.hash());
                                        }
                                        broadcast_blocks.push(block.hash());
                                        process_blocks.push(block.clone());
                                    } 
                                    //block isn't parent, keep orphan
                                    else { keep_orphans.push(orphan); }
                                }
                                //update orphan buffer with kept orphans & reset keep_orpans
                                orphan_buffer.orphans = keep_orphans.clone();
                                keep_orphans = Vec::<Block>::new();
                            }
                        }
                    }

                    if parent_blocks.len() != 0 {
                        peer.write(Message::GetBlocks(parent_blocks));
                    }
                    //https://piazza.com/class/kykjhx727ab1ge?cid=84
                    if broadcast_blocks.len() != 0 {
                        self.server.broadcast(Message::NewBlockHashes(broadcast_blocks));
                    }
                }
                Message::Transactions(txs) => {
                    let mut broadcast_transactions: Vec<H256> = Vec::<H256>::new();
                    let mut mempool = self.mempool.lock().unwrap();
                    for tx in txs {
                        if verify(&tx.transaction, &tx.public_key, &tx.signature) {
                            broadcast_transactions.push(tx.hash());
                            mempool.insert(&tx);
                        }
                    }

                    if broadcast_transactions.len() != 0 {
                        self.server.broadcast(Message::NewTransactionHashes(broadcast_transactions));
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
    let (server, server_receiver) = ServerHandle::new_for_test();
    let (test_msg_sender, msg_chan) = TestMsgSender::new();
    let blockchain = Blockchain::new();
    let blockchain = Arc::new(Mutex::new(blockchain));
    let mempool = Mempool::new();
    let mempool = Arc::new(Mutex::new(mempool));
    let tip = blockchain.lock().unwrap().tip();
    let block_state_map = Arc::new(Mutex::new(BlockState::new()));
    let worker = Worker::new(1, msg_chan, &server, &blockchain, &mempool, &block_state_map);
    worker.start(); 
    (test_msg_sender, server_receiver, vec![tip])
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
    //test with blocks already in the chain and new blocks together
    fn reply_new_block_hashes_more_blocks() {
        let (test_msg_sender, _server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut peer_receiver = test_msg_sender.send(Message::NewBlockHashes(vec![random_block.hash()]));
        let reply = peer_receiver.recv();
        if let Message::GetBlocks(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }

        let genesis: &H256 = v.last().unwrap();
        let block2 = generate_random_block(&random_block.hash());
        let block3 = generate_random_block(&block2.hash());
        let block4 = generate_random_block(&block3.hash());
        peer_receiver = test_msg_sender.send(Message::NewBlockHashes(vec![*genesis, random_block.hash(), block2.hash(), block3.hash(), block4.hash()]));
        let reply2 = peer_receiver.recv();
        if let Message::GetBlocks(v) = reply2 {
            assert_eq!(v, vec![random_block.hash(), block2.hash(), block3.hash(), block4.hash()]);
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
    //send blocks that don't exist in the chain
    fn reply_get_blocks_more_blocks() {
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

        let block2 = generate_random_block(&v.last().unwrap());
        let block3 = generate_random_block(&block2.hash());
        let block4 = generate_random_block(&block3.hash());
        peer_receiver = test_msg_sender.send(Message::GetBlocks(vec![h.clone(), block2.hash(), block3.hash(), block4.hash()]));
        let reply2 = peer_receiver.recv();
        if let Message::Blocks(v) = reply2 {
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
    #[test]
    #[timeout(60000)]
    //test sending blocks that are already in the chain and new blocks together
    fn reply_blocks_existing_blocks() {
        let (test_msg_sender, server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut _peer_receiver = test_msg_sender.send(Message::Blocks(vec![random_block.clone()]));
        let reply = server_receiver.recv().unwrap();
        if let Message::NewBlockHashes(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }

        let block2 = generate_random_block(&v.last().unwrap());
        let block3 = generate_random_block(&block2.hash());
        let block4 = generate_random_block(&block3.hash());
        _peer_receiver = test_msg_sender.send(Message::Blocks(vec![random_block.clone(), block2.clone(), block3.clone(), block4.clone()]));
        let reply2 = server_receiver.recv().unwrap();
        if let Message::NewBlockHashes(v) = reply2 {
            assert_eq!(v, vec![block2.hash(), block3.hash(), block4.hash()]);
        } else {
            panic!();
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST