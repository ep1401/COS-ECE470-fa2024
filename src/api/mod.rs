use serde::Serialize;
use crate::blockchain::Blockchain;
use crate::miner::Handle as MinerHandle;
use crate::network::server::Handle as NetworkServerHandle;
use crate::network::message::Message;

use crate::generator::generator::TransactionGenerator;
use crate::types::block::BlockState;
use crate::types::hash::{H256, Hashable};

use log::info;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::Header;
use tiny_http::Response;
use tiny_http::Server as HTTPServer;
use url::Url;

pub struct Server {
    handle: HTTPServer,
    miner: MinerHandle,
    tx_generator: TransactionGenerator,
    network: NetworkServerHandle,
    blockchain: Arc<Mutex<Blockchain>>,
    block_state: Arc<Mutex<BlockState>>
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

macro_rules! respond_result {
    ( $req:expr, $success:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let payload = ApiResponse {
            success: $success,
            message: $message.to_string(),
        };
        let resp = Response::from_string(serde_json::to_string_pretty(&payload).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}
macro_rules! respond_json {
    ( $req:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let resp = Response::from_string(serde_json::to_string(&$message).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}

impl Server {
    pub fn start(
        addr: std::net::SocketAddr,
        miner: &MinerHandle,
        tx_generator: &TransactionGenerator,
        network: &NetworkServerHandle,
        blockchain: &Arc<Mutex<Blockchain>>,
        block_state: &Arc<Mutex<BlockState>>
    ) {
        let handle = HTTPServer::http(&addr).unwrap();
        let server = Self {
            handle,
            miner: miner.clone(),
            tx_generator: tx_generator.clone(),
            network: network.clone(),
            blockchain: Arc::clone(blockchain),
            block_state: Arc::clone(block_state)
        };
        thread::spawn(move || {
            for req in server.handle.incoming_requests() {
                let miner = server.miner.clone();
                let tx_generator = server.tx_generator.clone();
                let network = server.network.clone();
                let blockchain = Arc::clone(&server.blockchain);
                let block_state_map = Arc::clone(&server.block_state);
                thread::spawn(move || {
                    // a valid url requires a base
                    let base_url = Url::parse(&format!("http://{}/", &addr)).unwrap();
                    let url = match base_url.join(req.url()) {
                        Ok(u) => u,
                        Err(e) => {
                            respond_result!(req, false, format!("error parsing url: {}", e));
                            return;
                        }
                    };
                    match url.path() {
                        "/miner/start" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let lambda = match params.get("lambda") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing lambda");
                                    return;
                                }
                            };
                            let lambda = match lambda.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lambda: {}", e)
                                    );
                                    return;
                                }
                            };
                            miner.start(lambda);
                            respond_result!(req, true, "ok");
                        }
                        "/tx-generator/start" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let theta = match params.get("theta") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing theta");
                                    return;
                                }
                            };
                            let theta = match theta.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing theta: {}", e)
                                    );
                                    return;
                                }
                            };
                            tx_generator.start(theta);
                            respond_result!(req, true, "ok");
                        }
                        "/network/ping" => {
                            network.broadcast(Message::Ping(String::from("Test ping")));
                            respond_result!(req, true, "ok");
                        }
                        "/blockchain/longest-chain" => {
                            let blockchain = blockchain.lock().unwrap();
                            let v = blockchain.all_blocks_in_longest_chain();
                            let v_string: Vec<String> = v.into_iter().map(|h|h.to_string()).collect();
                            respond_json!(req, v_string);
                        }
                        "/blockchain/longest-chain-tx" => {
                            let blockchain = blockchain.lock().unwrap();
                            let blocks = blockchain.all_blocks_in_longest_chain();
                            
                            let mut txs = Vec::<Vec<String>>::new();
                            
                            // Iterate over each block in the longest chain
                            for block_hash in blocks {
                                // Check if the block exists in the map (it should, since it's in the longest chain)
                                if let Some(block) = blockchain.blocks.get(&block_hash) {
                                    // Collect the transaction hashes in hex format for this block
                                    let tx_hashes: Vec<String> = block
                                        .content
                                        .transactions
                                        .iter()
                                        .map(|transaction| transaction.hash().to_string())
                                        .collect();
                                    // Add this block's transactions as a nested array to `txs`
                                    txs.push(tx_hashes);
                                }
                            }

                            // Send the JSON response
                            respond_json!(req, txs);
                        }
                        "/blockchain/longest-chain-tx-count" => {
                            // unimplemented!()
                            respond_result!(req, false, "unimplemented!");
                        }
                        // API handler for "/blockchain/state" route
                        "/blockchain/state" => {
                            // Extract the block parameter from the query string
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            
                            // Debugging: Print the received parameters
                            println!("Received parameters: {:?}", params);

                            let block = match params.get("block") {
                                Some(v) => v,
                                None => {
                                    println!("Missing block parameter");
                                    respond_result!(req, false, "missing block parameter");
                                    return;
                                }
                            };

                            // Parse the block parameter to a block number
                            let block_number = match block.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    println!("Error parsing block: {}", e);
                                    respond_result!(req, false, format!("error parsing block: {}", e));
                                    return;
                                }
                            };

                            // Debugging: Print the parsed block number
                            println!("Parsed block number: {}", block_number);

                            // Lock the blockchain and get the block hashes in the longest chain
                            let blockchain = blockchain.lock().unwrap();
                            let blocks_in_longest_chain = blockchain.all_blocks_in_longest_chain();

                            // Debugging: Print the length of the longest chain
                            println!("Longest chain length: {}", blocks_in_longest_chain.len());

                            // Check if the block number is within the bounds of the longest chain
                            if block_number < blocks_in_longest_chain.len() as u64 {
                                let block_hash = blocks_in_longest_chain[block_number as usize];
                                
                                // Debugging: Print the block hash
                                println!("Block hash at block number {}: {:?}", block_number, block_hash);

                                // Lock the block state map to retrieve the state for the specific block hash
                                let block_state_map = block_state_map.lock().unwrap();
                                println!("The length of block_state_map is: {}", block_state_map.block_state_map.len());
                                
                                if let Some(block_state) = block_state_map.block_state_map.get(&block_hash) {
                                    // Debugging: Print the block state
                                    println!("Block state found for block hash {:?}: {:?}", block_hash, block_state);

                                    // Format and return the state of the block
                                    let state: Vec<String> = block_state
                                        .iter()
                                        .map(|(address, (nonce, balance))| {
                                            format!("({}, {}, {})", address, nonce, balance)
                                        })
                                        .collect();
                                    
                                    respond_json!(req, state); // Respond with the block state as JSON
                                } else {
                                    println!("State not found for block hash {:?}", block_hash);
                                    respond_result!(req, false, "State not found for block");
                                }
                            } else {
                                println!("Block number {} is out of bounds", block_number);
                                respond_result!(req, false, "Block not found");
                            }
                        }

                        _ => {
                            let content_type =
                                "Content-Type: application/json".parse::<Header>().unwrap();
                            let payload = ApiResponse {
                                success: false,
                                message: "endpoint not found".to_string(),
                            };
                            let resp = Response::from_string(
                                serde_json::to_string_pretty(&payload).unwrap(),
                            )
                            .with_header(content_type)
                            .with_status_code(404);
                            req.respond(resp).unwrap();
                        }
                    }
                });
            }
        });
        info!("API server listening at {}", &addr);
    }
}
