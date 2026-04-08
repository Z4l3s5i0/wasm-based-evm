pub mod evm_rpc {
    tonic::include_proto!("evm_rpc");
}

use alloy_primitives::{Address, B256, FixedBytes, U256};
use alloy_rlp::{Encodable, RlpDecodable, RlpEncodable};
use alloy_trie::root::ordered_trie_root;
use clap::{Parser, Subcommand};
use evm_rpc::transaction_service_client::TransactionServiceClient;
use evm_rpc::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::str::FromStr;

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
#[rlp(trailing)]
pub struct Transaction {
    pub hash: B256,
    pub nonce: u64,
    pub from: Address,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub to: Option<Address>,
}

impl Transaction {
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }
}

fn random_b256() -> B256 {
    let mut buf = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut buf);
    FixedBytes::<32>(buf)
}

#[derive(Parser)]
#[command(name = "evm-tester")]
#[command(about = "A CLI tool to test the WASIX-based EVM gRPC server", no_binary_name = true)]
struct CommandCli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
#[command(name = "evm-tester")]
#[command(about = "A CLI tool to test the WASIX-based EVM gRPC server", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "http://127.0.0.1:50051")]
    server: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Returns a list of addresses owned by client
    Accounts,
    /// Returns the number of most recent block
    BlockNumber,
    /// Returns the current price per gas in wei
    GasPrice,
    /// Returns the balance of the account of given address
    GetBalance {
        address: String,
        #[arg(default_value = "latest")]
        block_tag: String,
    },
    /// Returns information about a block by number
    GetBlockByNumber {
        number: u64,
        #[arg(short, long)]
        full: bool,
    },
    /// Returns information about a block by hash
    GetBlockByHash {
        hash: String,
        #[arg(short, long)]
        full: bool,
    },
    /// Returns the number of transactions in a block by hash
    GetBlockTransactionCountByHash {
        hash: String,
    },
    /// Returns the number of transactions in a block by number
    GetBlockTransactionCountByNumber {
        number: u64,
    },
    /// Returns information about a transaction by hash
    GetTransactionByHash {
        hash: String,
    },
    GetTransactionReceipt {
        hash: String,
    },
    SendTransaction {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value = "0")]
        value: String,
        #[arg(long, default_value = "")]
        data: String,
        #[arg(long, default_value = "10000000")]
        gas_limit: u64,
        #[arg(long, default_value = "10")]
        gas_price: u64,
        #[arg(long, default_value = "0")]
        nonce: u64,
    },
    /// Executes a transaction (similar to sendTransaction for now)
    ExecuteTransaction {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value = "0")]
        value: String,
        #[arg(long, default_value = "")]
        data: String,
        #[arg(long, default_value = "10000000")]
        gas_limit: u64,
        #[arg(long, default_value = "1000000000")]
        gas_price: u64,
        #[arg(long, default_value = "0")]
        nonce: u64,
    },
    /// Executes a new message call immediately without creating a transaction
    EthCall {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long, default_value = "0")]
        value: String,
        #[arg(long, default_value = "")]
        data: String,
        #[arg(long, default_value = "10000000")]
        gas_limit: u64,
        #[arg(long, default_value = "1000000000")]
        gas_price: u64,
        #[arg(long, default_value = "0")]
        nonce: u64,
    },
    /// Returns the code at a given address
    GetCode {
        address: String,
        #[arg(long, default_value = "latest")]
        block_tag: String,
    },
    /// Returns the current trie roots for verification
    GetRoots,
    /// Returns the transactions currently in the mempool
    GetMempool,
    /// Starts a new block proposal
    BlockStart {
        #[arg(long)]
        slot: u64,
        #[arg(long)]
        parent_hash: Option<String>,
        #[arg(long)]
        timestamp: Option<u64>,
        #[arg(long)]
        fee_recipient: Option<String>,
    },
    /// Adds a transaction to the current block buffer
    BlockAddTx {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        value: String,
        #[arg(long, default_value = "")]
        data: String,
        #[arg(long, default_value = "21000")]
        gas_limit: u64,
        #[arg(long, default_value = "0")]
        gas_price: u64,
        #[arg(long, default_value = "0")]
        nonce: u64,
    },
    /// Proposes the current block buffer to the server
    BlockPropose {
        #[arg(long)]
        from_mempool: bool,
        #[arg(long, default_value = "100")]
        max_transactions: u32,
    },
    /// Clears the current block buffer
    BlockClear,
    /// Engine API: New Payload
    EngineNewPayload {
        #[arg(long)]
        parent_hash: String,
        #[arg(long)]
        fee_recipient: String,
        #[arg(long)]
        state_root: String,
        #[arg(long)]
        receipts_root: String,
        #[arg(long)]
        logs_bloom: String,
        #[arg(long)]
        prev_randao: String,
        #[arg(long)]
        block_number: u64,
        #[arg(long)]
        gas_limit: u64,
        #[arg(long)]
        gas_used: u64,
        #[arg(long)]
        timestamp: u64,
        #[arg(long)]
        block_hash: String,
        #[arg(long)]
        base_fee: String,
    },
    /// Engine API: Forkchoice Updated
    EngineForkchoiceUpdated {
        #[arg(long)]
        head: String,
        #[arg(long, default_value = "0x0000000000000000000000000000000000000000000000000000000000000000")]
        safe: String,
        #[arg(long, default_value = "0x0000000000000000000000000000000000000000000000000000000000000000")]
        finalized: String,
        #[arg(long)]
        timestamp: Option<u64>,
        #[arg(long)]
        prev_randao: Option<String>,
        #[arg(long)]
        suggested_fee_recipient: Option<String>,
    },
    /// Engine API: Get Payload
    EngineGetPayload {
        #[arg(long)]
        payload_id: String,
    },
    /// Opens the block explorer
    Explorer,
    Exit,
    NetPeerCount,
    NetPeers,
    NetAddPeer { addr: String },
    NetNodeInfo,
    /// Connects to a different EVM gRPC server
    Connect { addr: String },
}

async fn run_explorer(client: &mut TransactionServiceClient<tonic::transport::Channel>) -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Block Explorer ---");
    let mut rl = DefaultEditor::new()?;
    loop {
        let readline = rl.readline("explorer> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if line == "exit" || line == "quit" || line == "back" {
                    break;
                }
                rl.add_history_entry(line)?;

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }

                match parts[0] {
                    "blocks" => {
                        let latest = client.eth_block_number(Empty {}).await?.into_inner().number;
                        println!("Latest blocks (showing up to 10):");
                        let start = if latest > 10 { latest - 9 } else { 0 };
                        for n in (start..=latest).rev() {
                            let block = client.eth_get_block_by_number(GetBlockByNumberRequest {
                                number: n,
                                full_transactions: false,
                            }).await?.into_inner();
                            println!("Block #{}: hash={}, txs={}", block.number, block.hash, block.transactions.len());
                        }
                    }
                    "block" => {
                        if parts.len() < 2 {
                            println!("Usage: block <number>");
                            continue;
                        }
                        if let Ok(number) = parts[1].parse::<u64>() {
                            let block = client.eth_get_block_by_number(GetBlockByNumberRequest {
                                number,
                                full_transactions: true,
                            }).await?.into_inner();
                            println!("Block Information:");
                            println!("  Number: {}", block.number);
                            println!("  Hash: {}", block.hash);
                            println!("  Parent Hash: {}", block.parent_hash);
                            println!("  Timestamp: {}", block.timestamp);
                            println!("  Transactions ({}):", block.transactions.len());
                            for tx_hash in block.transactions {
                                println!("    - {}", tx_hash);
                            }
                        } else {
                            println!("Invalid block number");
                        }
                    }
                    "tx" => {
                        if parts.len() < 2 {
                            println!("Usage: tx <hash>");
                            continue;
                        }
                        let hash = parts[1].to_string();
                        let tx = client.eth_get_transaction_by_hash(GetTransactionByHashRequest {
                            hash,
                        }).await?.into_inner();
                        if tx.hash.is_empty() {
                            println!("Transaction not found");
                        } else {
                            println!("Transaction Information:");
                            println!("  Hash: {}", tx.hash);
                            println!("  From: {}", tx.from);
                            println!("  To: {}", tx.to);
                            println!("  Value: {} wei", tx.value);
                            println!("  Nonce: {}", tx.nonce);
                            println!("  Gas Limit: {}", tx.gas_limit);
                            println!("  Gas Price: {}", tx.gas_price);
                            println!("  Block: #{} ({})", tx.block_number, tx.block_hash);
                            if !tx.data.is_empty() {
                                println!("  Data: 0x{}", hex::encode(tx.data));
                            }
                        }
                    }
                    "help" => {
                        println!("Explorer commands:");
                        println!("  blocks      - List recent blocks");
                        println!("  block <n>   - Show details for block <n>");
                        println!("  tx <hash>   - Show details for transaction <hash>");
                        println!("  help        - Show this help");
                        println!("  exit/back   - Return to main menu");
                    }
                    _ => {
                        println!("Unknown command: {}. Type 'help' for help.", parts[0]);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut client = TransactionServiceClient::connect(cli.server).await?;
    let mut block_buffer: Option<(u64, Option<String>, Option<u64>, Option<String>, Vec<TransactionRequest>)> = None;

    println!("Connected to EVM gRPC server.");
    println!("Type 'help' for available commands, or 'exit' to quit.");

    let mut rl = DefaultEditor::new()?;

    loop {
        let readline = rl.readline("evm> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                rl.add_history_entry(line)?;

                let words = match shlex::split(line) {
                    Some(words) => words,
                    None => {
                        println!("Invalid input: unbalanced quotes");
                        continue;
                    }
                };

                let cmd_cli = match CommandCli::try_parse_from(words) {
                    Ok(c) => c,
                    Err(e) => {
                        println!("{}", e);
                        continue;
                    }
                };

                match cmd_cli.command {
                    Commands::Exit => break,
                    Commands::NetPeerCount => {
                        let response = client.net_peer_count(Empty {}).await?;
                        println!("Peer count: {}", response.into_inner().count);
                    }
                    Commands::NetPeers => {
                        let response = client.net_peers(Empty {}).await?;
                        println!("Peers: {:?}", response.into_inner().peers);
                    }
                    Commands::NetAddPeer { addr } => {
                        let response = client.net_add_peer(NetAddPeerRequest { addr }).await?;
                        let res = response.into_inner();
                        println!("Add Peer Response: success={}, message={}", res.success, res.message);
                    }
                    Commands::NetNodeInfo => {
                        let response = client.net_node_info(Empty {}).await?;
                        let res = response.into_inner();
                        println!("Node Info:");
                        println!("  ENR: {}", res.enr);
                        println!("  Node ID: {}", res.node_id);
                        println!("  Listen Addresses: {:?}", res.listen_addresses);
                    }
                    Commands::Connect { addr } => {
                        match TransactionServiceClient::connect(addr.clone()).await {
                            Ok(new_client) => {
                                client = new_client;
                                println!("Connected to EVM gRPC server at {}", addr);
                            }
                            Err(e) => {
                                println!("Failed to connect to {}: {}", addr, e);
                            }
                        }
                    }
                    Commands::Accounts => {
                        let response = client.eth_accounts(Empty {}).await?;
                        println!("Accounts: {:?}", response.into_inner().addresses);
                    }
                    Commands::BlockNumber => {
                        let response = client.eth_block_number(Empty {}).await?;
                        println!("Latest block number: {}", response.into_inner().number);
                    }
                    Commands::GasPrice => {
                        let response = client.eth_gas_price(Empty {}).await?;
                        println!("Current gas price: {} wei", response.into_inner().price);
                    }
                    Commands::GetBalance { address, block_tag } => {
                        let response = client.eth_get_balance(GetBalanceRequest {
                            address,
                            block_tag,
                        }).await?;
                        println!("Balance: {} wei", response.into_inner().balance);
                    }
                    Commands::GetBlockByNumber { number, full } => {
                        let response = client.eth_get_block_by_number(GetBlockByNumberRequest {
                            number,
                            full_transactions: full,
                        }).await?;
                        println!("Block: {:?}", response.into_inner());
                    }
                    Commands::GetBlockByHash { hash, full } => {
                        let response = client.eth_get_block_by_hash(GetBlockByHashRequest {
                            hash,
                            full_transactions: full,
                        }).await?;
                        println!("Block: {:?}", response.into_inner());
                    }
                    Commands::GetBlockTransactionCountByHash { hash } => {
                        let response = client.eth_get_block_transaction_count_by_hash(GetBlockTransactionCountByHashRequest {
                            hash,
                        }).await?;
                        println!("Transaction count: {}", response.into_inner().count);
                    }
                    Commands::GetBlockTransactionCountByNumber { number } => {
                        let response = client.eth_get_block_transaction_count_by_number(GetBlockTransactionCountByNumberRequest {
                            number,
                        }).await?;
                        println!("Transaction count: {}", response.into_inner().count);
                    }
                    Commands::GetTransactionByHash { hash } => {
                        let response = client.eth_get_transaction_by_hash(GetTransactionByHashRequest {
                            hash,
                        }).await?;
                        println!("Transaction: {:?}", response.into_inner());
                    }
                    Commands::GetTransactionReceipt { hash } => {
                        let response = client.eth_get_transaction_receipt(GetTransactionByHashRequest {
                            hash,
                        }).await?;
                        println!("Receipt: {:?}", response.into_inner());
                    }
                    Commands::SendTransaction { from, to, value, data, gas_limit, gas_price, nonce } => {
                        let data_bytes = hex::decode(data.trim_start_matches("0x")).unwrap_or_else(|_| data.into_bytes());
                        let response = client.eth_send_transaction(TransactionRequest {
                            from,
                            to: to.unwrap_or_default(),
                            value,
                            data: data_bytes,
                            gas_limit,
                            gas_price,
                            nonce,
                        }).await?;
                        let res = response.into_inner();
                        println!("Transaction Response: success={}, tx_hash={}", res.success, res.tx_hash);
                        if !res.contract_address.is_empty() {
                            println!("Contract Address: {}", res.contract_address);
                        }
                        if !res.return_data.is_empty() {
                            println!("Return Data: 0x{}", hex::encode(res.return_data));
                        }
                    }
                    Commands::ExecuteTransaction { from, to, value, data, gas_limit, gas_price, nonce } => {
                        let data_bytes = hex::decode(data.trim_start_matches("0x")).unwrap_or_else(|_| data.into_bytes());
                        let response = client.execute_transaction(TransactionRequest {
                            from,
                            to: to.unwrap_or_default(),
                            value,
                            data: data_bytes,
                            gas_limit,
                            gas_price,
                            nonce,
                        }).await?;
                        let res = response.into_inner();
                        println!("Transaction Response: success={}, tx_hash={}", res.success, res.tx_hash);
                        if !res.contract_address.is_empty() {
                            println!("Contract Address: {}", res.contract_address);
                        }
                        if !res.return_data.is_empty() {
                            println!("Return Data: 0x{}", hex::encode(res.return_data));
                        }
                    }
                    Commands::EthCall { from, to, value, data, gas_limit, gas_price, nonce } => {
                        let data_bytes = hex::decode(data.trim_start_matches("0x")).unwrap_or_else(|_| data.into_bytes());
                        let response = client.eth_call(TransactionRequest {
                            from,
                            to: to.unwrap_or_default(),
                            value,
                            data: data_bytes,
                            gas_limit,
                            gas_price,
                            nonce,
                        }).await?;
                        let res = response.into_inner();
                        println!("Call Response: success={}", res.success);
                        if !res.return_data.is_empty() {
                            println!("Return Data: 0x{}", hex::encode(res.return_data));
                        }
                    }
                    Commands::GetCode { address, block_tag } => {
                        let response = client.eth_get_code(GetCodeRequest {
                            address,
                            block_tag,
                        }).await?;
                        println!("Code: 0x{}", hex::encode(response.into_inner().code));
                    }
                    Commands::GetRoots => {
                        let response = client.eth_get_roots(Empty {}).await?;
                        let res = response.into_inner();
                        println!("Latest Roots:");
                        println!("  State Root:        {}", res.state_root);
                        println!("  Transactions Root: {}", res.transactions_root);
                        println!("  Receipts Root:     {}", res.receipts_root);
                        println!("  Withdrawals Root:  {}", res.withdrawals_root);
                    }
                    Commands::GetMempool => {
                        let response = client.eth_get_mempool(Empty {}).await?;
                        let res = response.into_inner();
                        println!("Mempool Transactions ({}):", res.transactions.len());
                        for tx in res.transactions {
                            println!("  Hash: {}, From: {}, To: {}, Value: {}", tx.hash, tx.from, tx.to, tx.value);
                        }
                    }
                    Commands::BlockStart { slot, parent_hash, timestamp, fee_recipient } => {
                        block_buffer = Some((slot, parent_hash, timestamp, fee_recipient, Vec::new()));
                        println!("Block proposal started for slot {}.", slot);
                    }
                    Commands::BlockAddTx { from, to, value, data, gas_limit, gas_price, nonce } => {
                        if let Some(ref mut buffer) = block_buffer {
                            let tx = TransactionRequest {
                                from,
                                to: to.unwrap_or_default(),
                                value,
                                data: if data.starts_with("0x") { hex::decode(&data[2..])? } else { data.into_bytes() },
                                gas_limit,
                                gas_price,
                                nonce,
                            };
                            buffer.4.push(tx);
                            println!("Transaction added to block buffer (total: {}).", buffer.4.len());
                        } else {
                            println!("No block proposal in progress. Start one with 'block-start'.");
                        }
                    }
                    Commands::BlockPropose { from_mempool, max_transactions } => {
                        if let Some((slot, parent_hash, timestamp, fee_recipient, transactions)) = block_buffer.take() {
                            let request = ProposeBlockRequest {
                                slot,
                                parent_hash: parent_hash.unwrap_or_default(),
                                transactions,
                                timestamp: timestamp.unwrap_or_default(),
                                fee_recipient: fee_recipient.unwrap_or_default(),
                                from_mempool,
                                max_transactions,
                            };
                            let response = client.propose_block(request).await?;
                            let res = response.into_inner();
                            println!("Block Proposal Response: success={}, block_hash={}, message={}", res.success, res.block_hash, res.message);
                            for (i, tx_res) in res.tx_results.iter().enumerate() {
                                println!("  Tx {}: success={}, message={}", i, tx_res.success, tx_res.message);
                            }
                        } else if from_mempool {
                            // If no buffer but from_mempool is true, we can still propose a block
                            let request = ProposeBlockRequest {
                                slot: 0, // Server will decide slot if not provided or we could ask for latest block + 1
                                parent_hash: String::new(),
                                transactions: Vec::new(),
                                timestamp: 0,
                                fee_recipient: String::new(),
                                from_mempool,
                                max_transactions,
                            };
                            let response = client.propose_block(request).await?;
                            let res = response.into_inner();
                            println!("Block Proposal (Mempool) Response: success={}, block_hash={}, message={}", res.success, res.block_hash, res.message);
                            for (i, tx_res) in res.tx_results.iter().enumerate() {
                                println!("  Tx {}: success={}, message={}", i, tx_res.success, tx_res.message);
                            }
                        } else {
                            println!("No block proposal in progress. Use --from-mempool to propose from mempool.");
                        }
                    }
                    Commands::BlockClear => {
                        block_buffer = None;
                        println!("Block buffer cleared.");
                    }
                    Commands::EngineNewPayload {
                        parent_hash,
                        fee_recipient,
                        state_root,
                        receipts_root,
                        logs_bloom,
                        prev_randao,
                        block_number,
                        gas_limit,
                        gas_used,
                        timestamp,
                        block_hash,
                        base_fee,
                    } => {
                        let mut encoded_transactions = Vec::new();
                        let mut transaction_objs = Vec::new();

                        if let Some(ref buffer) = block_buffer {
                            for tx_req in &buffer.4 {
                                let from = Address::from_str(&tx_req.from).unwrap_or_default();
                                let to = if tx_req.to.is_empty() {
                                    None
                                } else {
                                    Some(Address::from_str(&tx_req.to).unwrap_or_default())
                                };
                                let value = U256::from_str(&tx_req.value).unwrap_or_default();
                                let gas_limit = tx_req.gas_limit;
                                let gas_price = U256::from(tx_req.gas_price);
                                let nonce = tx_req.nonce;
                                let data = tx_req.data.clone();

                                let tx = Transaction {
                                    hash: random_b256(), // For testing, we generate a random hash
                                    nonce,
                                    from,
                                    value,
                                    data,
                                    gas_limit,
                                    gas_price,
                                    to,
                                };

                                encoded_transactions.push(tx.to_vec());
                                transaction_objs.push(tx);
                            }
                        }

                        // Calculate transactions root using alloy-trie
                        let tx_root = ordered_trie_root(&transaction_objs);

                        let request = ExecutionPayload {
                            parent_hash,
                            fee_recipient,
                            state_root,
                            receipts_root,
                            logs_bloom,
                            prev_randao,
                            block_number,
                            gas_limit,
                            gas_used,
                            timestamp,
                            extra_data: Vec::new(),
                            base_fee_per_gas: base_fee,
                            block_hash,
                            transactions: encoded_transactions,
                            withdrawals: Vec::new(),
                            blob_gas_used: 0,
                            excess_blob_gas: 0,
                            transactions_root: format!("{:?}", tx_root),
                            withdrawals_root: String::new(),
                        };
                        let response = client.engine_new_payload(request).await?;
                        let res = response.into_inner();
                        println!("Engine New Payload Response: status={}, latest_valid_hash={}, error={}", res.status, res.latest_valid_hash, res.validation_error);
                    }
                    Commands::EngineForkchoiceUpdated {
                        head,
                        safe,
                        finalized,
                        timestamp,
                        prev_randao,
                        suggested_fee_recipient,
                    } => {
                        let payload_attributes = if let Some(t) = timestamp {
                            Some(PayloadAttributes {
                                timestamp: t,
                                prev_randao: prev_randao.unwrap_or_default(),
                                suggested_fee_recipient: suggested_fee_recipient.unwrap_or_default(),
                                withdrawals: Vec::new(),
                                parent_beacon_block_root: String::new(),
                            })
                        } else {
                            None
                        };
                        let request = ForkchoiceUpdatedRequest {
                            forkchoice_state: Some(ForkchoiceState {
                                head_block_hash: head,
                                safe_block_hash: safe,
                                finalized_block_hash: finalized,
                            }),
                            payload_attributes,
                        };
                        let response = client.engine_forkchoice_updated(request).await?;
                        let res = response.into_inner();
                        let status = res.payload_status.unwrap_or_default();
                        println!("Engine Forkchoice Updated Response: status={}, latest_valid_hash={}, payload_id={}", status.status, status.latest_valid_hash, res.payload_id);
                    }
                    Commands::EngineGetPayload { payload_id } => {
                        let request = GetPayloadRequest { payload_id };
                        let response = client.engine_get_payload(request).await?;
                        let res = response.into_inner();
                        println!("Engine Get Payload Response:");
                        println!("  Block Number: {}", res.block_number);
                        println!("  Block Hash:   {}", res.block_hash);
                        println!("  Parent Hash:  {}", res.parent_hash);
                        println!("  Timestamp:    {}", res.timestamp);
                        println!("  Transactions: {}", res.transactions.len());
                    }
                    Commands::Explorer => {
                        run_explorer(&mut client).await?;
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}