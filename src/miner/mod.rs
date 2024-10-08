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

pub mod worker;

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

pub struct Context {
    /// Channel for receiving control signal
    control_chan: Receiver<ControlSignal>,
    operating_state: OperatingState,
    finished_block_chan: Sender<Block>,
    blockchain: Arc<Mutex<Blockchain>>,
}

#[derive(Clone)]
pub struct Handle {
    /// Channel for sending signal to the miner thread
    control_chan: Sender<ControlSignal>,
}

pub fn new(blockchain: Arc<Mutex<Blockchain>>) -> (Context, Handle, Receiver<Block>) {
    let (signal_chan_sender, signal_chan_receiver) = unbounded();
    let (finished_block_sender, finished_block_receiver) = unbounded();

    let ctx = Context {
        control_chan: signal_chan_receiver,
        operating_state: OperatingState::Paused,
        finished_block_chan: finished_block_sender,
        blockchain: Arc::clone(&blockchain),
    };

    let handle = Handle {
        control_chan: signal_chan_sender,
    };

    (ctx, handle, finished_block_receiver)
}

#[cfg(any(test,test_utilities))]
fn test_new() -> (Context, Handle, Receiver<Block>) {
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));
    new(blockchain)
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

            // TODO for student: actual mining, create a block
            // TODO for student: if block mining finished, you can have something like: self.finished_block_chan.send(block.clone()).expect("Send finished block error");
            // Mining logic begins here
            // Mining logic begins here
            let (parent_, parent_block, difficulty_) = {
                // Lock the blockchain and get the parent block information
                let blockchain = self.blockchain.lock().unwrap();
                let parent_ = blockchain.tip(); // Get the current tip of the blockchain
                let parent_block = blockchain.blocks.get(&parent_).expect("Parent block not found");
                let difficulty_ = parent_block.header.difficulty;
                (parent_, parent_block.clone(), difficulty_)
            };

            let start = SystemTime::now();
            let mut rng = rand::thread_rng();
            let timestamp_ = start
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();

            let transactions = Vec::<SignedTransaction>::new();  // Empty transactions for now
            let merkle_tree_ = MerkleTree::new(&transactions);

            // Mining loop: try different nonces
            loop {
                let nonce_ = rng.gen::<u32>();  // Generate new nonce
                let header_ = Header {
                    parent: parent_,
                    nonce: nonce_,
                    difficulty: difficulty_,
                    timestamp: timestamp_,
                    merkle_root: merkle_tree_.root(),
                };
                let block = Block {
                    header: header_,
                    content: Content {
                        transactions: transactions.clone(),
                    },
                };

                // Check if the block satisfies the proof-of-work condition
                if block.hash() <= difficulty_ {
                    // Insert the mined block into the blockchain
                    {
                        let mut blockchain = self.blockchain.lock().unwrap();
                        blockchain.insert(&block);
                    }

                    // Send the block to the finished_block_chan
                    self.finished_block_chan
                        .send(block.clone())
                        .expect("Send finished block error");

                    break;  // Block mined, break out of loop to mine the next block
                }
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