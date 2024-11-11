use crate::types::block::{Block, Header, Content};
use crate::types::hash::H256;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use rand::Rng;
use log::info;
use crate::blockchain::Blockchain;
use crate::types::hash::Hashable;
use crate::types::merkle::MerkleTree;
use crate::types::transaction::SignedTransaction;
use crate::blockchain::DIFFICULTY;

pub mod worker;

use std::collections::HashSet;
use crate::types::block::BlockState;

use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use std::thread;
use std::time;

enum ControlSignal {
    Start(u64), // the number controls the lambda of interval between block generation
    Update, // update the block in mining, it may due to new blockchain tip or new transaction
    Exit,
}

enum OperatingState {
    Paused,
    Run(u64),
    ShutDown,
}

pub struct Mempool {
    //map is used to store Txs not added yet to the blockchain
    pub transaction_map: HashMap<H256, SignedTransaction>,
    //set is used as a record for all transactions added to blockchain
    pub transaction_set: HashSet<H256>
}
//implement Mempool like Blockchain
impl Mempool {
    pub fn new() -> Self {
        return Mempool {
            transaction_map: HashMap::<H256, SignedTransaction>::new(),
            transaction_set: HashSet::<H256>::new()
        }
    }

    pub fn insert(&mut self, transaction: &SignedTransaction) {
        if self.transaction_set.contains(&transaction.hash()) {
            return;
        }
        self.transaction_map.insert(transaction.hash(), transaction.clone());
        self.transaction_set.insert(transaction.hash());
        println!("Mempool - Inserting transaction: {:?}", transaction.hash());
    }

    pub fn remove(&mut self, transaction_hash: &H256) {
        if self.transaction_map.contains_key(&transaction_hash) {
            self.transaction_map.remove(&transaction_hash);
        }
    }
}

pub struct Context {
    /// Channel for receiving control signal
    control_chan: Receiver<ControlSignal>,
    operating_state: OperatingState,
    finished_block_chan: Sender<Block>,
    blockchain: Arc<Mutex<Blockchain>>,
    mempool: Arc<Mutex<Mempool>>,
    block_state_map: Arc<Mutex<BlockState>>,
}

#[derive(Clone)]
pub struct Handle {
    /// Channel for sending signal to the miner thread
    control_chan: Sender<ControlSignal>,
}

pub fn new(blockchain: Arc<Mutex<Blockchain>>, mempool: &Arc<Mutex<Mempool>>, 
    block_state_map: &Arc<Mutex<BlockState>>) -> (Context, Handle, Receiver<Block>) {
    let (signal_chan_sender, signal_chan_receiver) = unbounded();
    let (finished_block_sender, finished_block_receiver) = unbounded();

    let ctx = Context {
        control_chan: signal_chan_receiver,
        operating_state: OperatingState::Paused,
        finished_block_chan: finished_block_sender,
        blockchain: Arc::clone(&blockchain),
        mempool: Arc::clone(mempool),
        block_state_map: Arc::clone(block_state_map),
    };

    let handle = Handle {
        control_chan: signal_chan_sender,
    };

    (ctx, handle, finished_block_receiver)
}

#[cfg(any(test,test_utilities))]
fn test_new() -> (Context, Handle, Receiver<Block>) {
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));
    let mempool = Arc::new(Mutex::new(Mempool::new()));
    let block_state_map = Arc::new(Mutex::new(BlockState::new()));
    new(blockchain, &mempool, &block_state_map)
}

impl Handle {
    pub fn exit(&self) {
        self.control_chan.send(ControlSignal::Exit).unwrap();
    }

    pub fn start(&self, lambda: u64) {
        self.control_chan
            .send(ControlSignal::Start(lambda))
            .unwrap();
    }

    pub fn update(&self) {
        self.control_chan.send(ControlSignal::Update).unwrap();
    }
}

impl Context {
    pub fn start(mut self) {
        thread::Builder::new()
            .name("miner".to_string())
            .spawn(move || {
                self.miner_loop();
            })
            .unwrap();
        info!("Miner initialized into paused mode");
    }

    fn miner_loop(&mut self) {
        loop {
            // Check and handle control signals
            match self.operating_state {
                OperatingState::Paused => {
                    let signal = self.control_chan.recv().unwrap();
                    match signal {
                        ControlSignal::Exit => {
                            info!("Miner shutting down");
                            self.operating_state = OperatingState::ShutDown;
                        }
                        ControlSignal::Start(i) => {
                            info!("Miner starting with lambda {}", i);
                            self.operating_state = OperatingState::Run(i);
                        }
                        ControlSignal::Update => {
                            // No action needed in paused state
                        }
                    };
                    continue;
                }
                OperatingState::ShutDown => return,
                _ => match self.control_chan.try_recv() {
                    Ok(signal) => match signal {
                        ControlSignal::Exit => {
                            info!("Miner shutting down");
                            self.operating_state = OperatingState::ShutDown;
                        }
                        ControlSignal::Start(i) => {
                            info!("Miner starting with lambda {}", i);
                            self.operating_state = OperatingState::Run(i);
                        }
                        ControlSignal::Update => {
                            // No update logic yet
                            println!("Miner received update signal, pausing to update state");
                            continue;
                        }
                    },
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => panic!("Miner control channel detached"),
                },
            }
    
            if let OperatingState::ShutDown = self.operating_state {
                return;
            }
    
            // Get the current blockchain tip
            let parent = self.blockchain.lock().unwrap().tip();
            let start = SystemTime::now();
            let mut rng = rand::thread_rng();
            let timestamp = start
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            let difficulty: H256 = DIFFICULTY.into();
    
            // Clone the transaction map from the mempool to avoid holding the lock for long
            let transaction_map;
            {
                let mempool = self.mempool.lock().unwrap();
                transaction_map = mempool.transaction_map.clone();
            }
    
            // Collect transactions up to the block size limit
            let mut transactions = Vec::<SignedTransaction>::new();
            // let block_limit = 10000;
            let block_limit = 60000;
            let mut current_size = 0;
    
            for (_, tx) in transaction_map.iter() {
                let bytes = bincode::serialize(&tx).unwrap();
                if current_size + bytes.len() > block_limit {
                    break;
                }
                current_size += bytes.len();
                transactions.push(tx.clone());
            }

            // println!("Miner - Total transactions added to block: {}", transactions.len());
    
            // Construct the block
            let merkle_tree = MerkleTree::new(&transactions);
            let nonce = rng.gen::<u32>();
            let header = Header {
                parent,
                nonce,
                difficulty,
                timestamp,
                merkle_root: merkle_tree.root(),
            };
            let content = Content { transactions };
            let block = Block { header, content };
    
            // Check if the block meets the difficulty target
            if block.hash() <= difficulty {
                // Remove included transactions from the mempool
                {
                    let mut mempool = self.mempool.lock().expect("Failed to lock mempool");
                    for tx in &block.content.transactions {
                        mempool.remove(&tx.hash());
                    }
                }
    
                // Send the finished block
                self.finished_block_chan
                    .send(block.clone())
                    .expect("Failed to send finished block");
            }
    
            // Control the mining interval based on the lambda value
            if let OperatingState::Run(lambda) = self.operating_state {
                if lambda != 0 {
                    thread::sleep(time::Duration::from_micros(lambda));
                }
            }
        }
    }

}
    

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod test {
    use ntest::timeout;
    use crate::types::hash::Hashable;

    #[test]
    #[timeout(60000)]
    fn miner_three_block() {
        let (miner_ctx, miner_handle, finished_block_chan) = super::test_new();
        miner_ctx.start();
        miner_handle.start(0);
        let mut block_prev = finished_block_chan.recv().unwrap();
        for _ in 0..2 {
            let block_next = finished_block_chan.recv().unwrap();
            assert_eq!(block_prev.hash(), block_next.get_parent());
            block_prev = block_next;
        }
    }

    
    /*
    #[timeout(60000)]
    fn miner_ten_block() {
        let (miner_ctx, miner_handle, finished_block_chan) = super::test_new();
        miner_ctx.start();
        miner_handle.start(0);
        let mut block_prev = finished_block_chan.recv().unwrap();
        for _ in 0..9 {
            let block_next = finished_block_chan.recv().unwrap();
            assert_eq!(block_prev.hash(), block_next.get_parent());
            block_prev = block_next;
        }
    }
    */
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST