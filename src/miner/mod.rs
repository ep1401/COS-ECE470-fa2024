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
        // main mining loop
        loop {
            // check and react to control signals
            match self.operating_state {
                OperatingState::Paused => {
                    let signal = self.control_chan.recv().unwrap();
                    match signal {
                        ControlSignal::Exit => {
                            info!("Miner shutting down");
                            self.operating_state = OperatingState::ShutDown;
                        }
                        ControlSignal::Start(i) => {
                            info!("Miner starting in continuous mode with lambda {}", i);
                            self.operating_state = OperatingState::Run(i);
                        }
                        ControlSignal::Update => {
                            // in paused state, don't need to update
                        }
                    };
                    continue;
                }
                OperatingState::ShutDown => {
                    return;
                }
                _ => match self.control_chan.try_recv() {
                    Ok(signal) => {
                        match signal {
                            ControlSignal::Exit => {
                                info!("Miner shutting down");
                                self.operating_state = OperatingState::ShutDown;
                            }
                            ControlSignal::Start(i) => {
                                info!("Miner starting in continuous mode with lambda {}", i);
                                self.operating_state = OperatingState::Run(i);
                            }
                            ControlSignal::Update => {
                                unimplemented!()
                            }
                        };
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => panic!("Miner control channel detached"),
                },
            }
            if let OperatingState::ShutDown = self.operating_state {
                return;
            }
            let parent_ = self.blockchain.lock().unwrap().tip();
            let start = SystemTime::now();
            let mut rng = rand::thread_rng();
            let timestamp_ = start.duration_since(UNIX_EPOCH).expect("Time went backwards").as_millis();
            let difficulty_: H256 = DIFFICULTY.into();
            let mut tip_state = self.block_state_map.lock().unwrap().block_state_map.get(&parent_).unwrap().clone();
            /////////Transaction Logic - add transactions from mempool to block/////////
            let mut transactions = Vec::<SignedTransaction>::new();
            let mut mempool = self.mempool.lock().unwrap();
            let block_limit = 4000;
            let mut current_size = 0;
            let mut bytes: Vec<u8>;
            for (_, tx) in mempool.transaction_map.clone().iter() {
                bytes = bincode::serialize(&tx).unwrap();
                if current_size + bytes.len() > block_limit {
                    break;
                }
                /////////State checks///////////
                let transaction = &tx.transaction;
                let sender_state;
                if tip_state.contains_key(&transaction.sender) {
                    sender_state = tip_state.get(&transaction.sender).unwrap().clone();
                } else {
                    sender_state = (0, 0);
                }
                if (transaction.value > sender_state.1) || (transaction.account_nonce != sender_state.0 + 1) {
                    //remove Txs with nonce lower than current, otherwise keep (out-of-order Txs, etc.)
                    if transaction.account_nonce <= sender_state.0 {
                        println!("Sender balance: {:?}", sender_state.1);
                        println!("Sender state: {:?}", sender_state.0);
                        println!("Account nonce: {:?}", transaction.account_nonce);
                        println!("Transaction with lower nonce than current state, removing from mempool");
                        mempool.remove(&tx.hash());
                    }
                    continue;
                }
                //at this point the transaction is valid so update local state copy
                tip_state.insert(transaction.sender, (sender_state.0 + 1, sender_state.1 - &transaction.value));
                let receiver_state;
                if tip_state.contains_key(&transaction.receiver) {
                    receiver_state = tip_state.get(&transaction.receiver).unwrap().clone();
                } else {
                    receiver_state = (0, 0);
                }
                tip_state.insert(transaction.receiver, (receiver_state.0, receiver_state.1 + &transaction.value));
                ////////////////////////////////
                current_size += bytes.len();
                let x = &*tx;
                transactions.push(x.clone());
            }
            ////////////////////////////////////////////////////////////////////////////

            let merkle_tree_ = MerkleTree::new(&transactions);
            let nonce_ = rng.gen::<u32>();
            let header_ = Header {
                parent: parent_,
                nonce: nonce_,
                difficulty: difficulty_,
                timestamp: timestamp_,
                merkle_root: merkle_tree_.root()
            };
            let content_ = Content {
                transactions: transactions
            };
            let block = Block {
                header: header_,
                content: content_
            };
            if block.hash() <= difficulty_ {
                //Remove transactions from mempool
                for tx in block.content.transactions.clone() {
                    mempool.remove(&tx.hash());
                }
                //add block to block state
                self.block_state_map.lock().unwrap().block_state_map.insert(block.hash(), tip_state.clone());
                //Remove invalid transactions after state update
                for (_, tx) in mempool.transaction_map.clone().iter() {
                    let sender = tx.transaction.sender;
                    let sender_state = tip_state.get(&sender).unwrap().clone();
                    if (tx.transaction.value > sender_state.1) || (tx.transaction.account_nonce != sender_state.0 + 1) {
                        if tx.transaction.account_nonce <= sender_state.0 {
                            mempool.remove(&tx.hash());
                        }
                    }
                }
                self.finished_block_chan.send(block.clone()).expect("Send finished block error");
            }

            if let OperatingState::Run(i) = self.operating_state {
                if i != 0 {
                    let interval = time::Duration::from_micros(i as u64);
                    thread::sleep(interval);
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