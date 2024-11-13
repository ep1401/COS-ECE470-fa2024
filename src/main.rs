#[cfg(test)]
#[macro_use]
extern crate hex_literal;

pub mod api;
pub mod blockchain;
pub mod types;
pub mod miner;
pub mod network;
pub mod generator;

use blockchain::Blockchain;
use clap::clap_app;
use miner::Mempool;
use ring::signature::KeyPair;
use smol::channel;
use log::{error, info};
use api::Server as ApiServer;
use types::transaction::ICO;
use std::net;
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;

use crate::types::address::Address;
use crate::types::block::BlockState;
use crate::types::key_pair::given;
use crossbeam::channel::{unbounded};

fn main() {
    // Parse command line arguments
    let matches = clap_app!(Bitcoin =>
        (version: "0.1")
        (about: "Bitcoin client")
        (@arg verbose: -v ... "Increases the verbosity of logging")
        (@arg peer_addr: --p2p [ADDR] default_value("127.0.0.1:6000") "Sets the IP address and the port of the P2P server")
        (@arg api_addr: --api [ADDR] default_value("127.0.0.1:7000") "Sets the IP address and the port of the API server")
        (@arg known_peer: -c --connect ... [PEER] "Sets the peers to connect to at start")
        (@arg p2p_workers: --("p2p-workers") [INT] default_value("4") "Sets the number of worker threads for P2P server")
    )
    .get_matches();

    // Initialize logger
    let verbosity = matches.occurrences_of("verbose") as usize;
    stderrlog::new().verbosity(verbosity).init().unwrap();

    // Initialize blockchain and mempool
    let blockchain = Arc::new(Mutex::new(Blockchain::new()));
    let mempool = Arc::new(Mutex::new(Mempool::new()));

    // Create key-pairs for nodes
    let pair0 = Arc::new(given(&[0; 32]));
    let account0 = Address::from_public_key_bytes(pair0.public_key().as_ref());

    let pair1 = Arc::new(given(&[1; 32]));
    let account1 = Address::from_public_key_bytes(pair1.public_key().as_ref());

    let pair2 = Arc::new(given(&[2; 32]));
    let account2 = Address::from_public_key_bytes(pair2.public_key().as_ref());

    // Initialize state map with ICO initial balances and nonces
    let mut initial_state = std::collections::HashMap::new();
    initial_state.insert(account0, (0, 1_000_000));
    let state_map = Arc::new(Mutex::new(initial_state));

    let ico = Arc::new(Mutex::new(ICO::new(pair0.public_key().as_ref())));

    let block_state_map = Arc::new(Mutex::new(BlockState::new()));
    let genesis_hash = blockchain.lock().unwrap().tip();
    block_state_map.lock().unwrap().block_state_map.insert(genesis_hash, ico.lock().unwrap().state.clone());

    // Parse P2P server address
    let p2p_addr = matches
        .value_of("peer_addr")
        .unwrap()
        .parse::<net::SocketAddr>()
        .unwrap_or_else(|e| {
            error!("Error parsing P2P server address: {}", e);
            process::exit(1);
        });

    // Parse API server address
    let api_addr = matches
        .value_of("api_addr")
        .unwrap()
        .parse::<net::SocketAddr>()
        .unwrap_or_else(|e| {
            error!("Error parsing API server address: {}", e);
            process::exit(1);
        });

    // Create channels between server and worker
    let (msg_tx, msg_rx) = channel::bounded(10000);

    // Start the P2P server
    let (server_ctx, server) = network::server::new(p2p_addr, msg_tx).unwrap();
    server_ctx.start().unwrap();

    // Start the worker
    let p2p_workers = matches
        .value_of("p2p_workers")
        .unwrap()
        .parse::<usize>()
        .unwrap_or_else(|e| {
            error!("Error parsing P2P workers: {}", e);
            process::exit(1);
        });
    let worker_ctx = network::worker::Worker::new(
        p2p_workers,
        msg_rx,
        &server,
        &blockchain,
        &mempool,
        &block_state_map,
    );
    worker_ctx.start();

    // Choose which account to use based on the port
    let address_to_use = p2p_addr.port() % 10;
    let (chosen_address, chosen_keypair, receiver_addresses) = match address_to_use {
        1 => (account1, Arc::clone(&pair1), [account0, account2]),
        2 => (account2, Arc::clone(&pair2), [account0, account1]),
        _ => (account0, Arc::clone(&pair0), [account1, account2]),
    };

    // Initialize the TransactionGenerator
    let (finished_tx_sender, finished_tx_receiver) = unbounded();
    let transaction_generator = generator::generator::TransactionGenerator::new(
        Arc::clone(&blockchain),
        chosen_address.clone(),
        Arc::clone(&chosen_keypair),
        Arc::clone(&block_state_map),
        receiver_addresses.clone(),
        server.clone(),
        Arc::clone(&mempool),
        finished_tx_sender,
    );
    // transaction_generator.clone().start(100);

    // Start the miner
    let (miner_ctx, miner, finished_block_chan) = miner::new(
        Arc::clone(&blockchain),
        &mempool,
        &block_state_map,
    );
    let miner_worker_ctx = miner::worker::Worker::new(&server, finished_block_chan, Arc::clone(&blockchain));
    miner_ctx.start();
    miner_worker_ctx.start();

    // Connect to known peers
    if let Some(known_peers) = matches.values_of("known_peer") {
        let known_peers: Vec<String> = known_peers.map(|x| x.to_owned()).collect();
        let server = server.clone();
        thread::spawn(move || {
            for peer in known_peers {
                loop {
                    let addr = match peer.parse::<net::SocketAddr>() {
                        Ok(x) => x,
                        Err(e) => {
                            error!("Error parsing peer address {}: {}", &peer, e);
                            break;
                        }
                    };
                    match server.connect(addr) {
                        Ok(_) => {
                            info!("Connected to outgoing peer {}", &addr);
                            break;
                        }
                        Err(e) => {
                            error!("Error connecting to peer {}, retrying in one second: {}", addr, e);
                            thread::sleep(time::Duration::from_millis(1000));
                        }
                    }
                }
            }
        });
    }

    // Start the API server
    ApiServer::start(
        api_addr,
        &miner,
        &transaction_generator,
        &server,
        &blockchain,
        &block_state_map,
    );

    // Main loop to keep the application running
    loop {
        std::thread::park();
    }
}
