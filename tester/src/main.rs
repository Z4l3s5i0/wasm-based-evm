use alloy_primitives::{Address, Bytes};
use alloy_consensus::Transaction as _;
use alloy_rpc_types::{Block, Transaction, TransactionReceipt, Filter, Log};
use alloy_eips::BlockId;
use clap::{Parser, Subcommand};
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::str::FromStr;

#[derive(Parser)]
#[command(name = "evm-tester")]
#[command(about = "A CLI tool to test the WASIX-based EVM JSON-RPC server", no_binary_name = true)]
struct CommandCli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser)]
#[command(name = "evm-tester")]
#[command(about = "A CLI tool to test the WASIX-based EVM JSON-RPC server", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "http://127.0.0.1:50051")]
    server: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Returns the number of most recent block
    BlockNumber,
    /// Returns the balance of the account of given address
    GetBalance {
        address: String,
        #[arg(default_value = "latest")]
        block_tag: String,
    },
    /// Returns the number of transactions sent from an address
    GetTransactionCount {
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
    /// Returns information about a transaction by hash
    GetTransactionByHash {
        hash: String,
    },
    /// Returns the receipt of a transaction by hash
    GetTransactionReceipt {
        hash: String,
    },
    /// Returns logs matching the given filter
    GetLogs {
        #[arg(long)]
        address: Option<String>,
        #[arg(long)]
        from_block: Option<String>,
        #[arg(long)]
        to_block: Option<String>,
        #[arg(long)]
        topics: Vec<String>,
    },
    /// Opens the block explorer
    #[command(about = "Get account code")]
    GetCode {
        #[arg(short, long)]
        address: String,
        #[arg(short, long, default_value = "latest")]
        block_tag: String,
    },
    #[command(about = "Get storage at a given slot")]
    GetStorageAt {
        #[arg(short, long)]
        address: String,
        #[arg(short, long)]
        slot: String,
        #[arg(short, long, default_value = "latest")]
        block_tag: String,
    },
    #[command(about = "Get chain ID")]
    ChainId,
    #[command(about = "Get gas price")]
    GasPrice,
    #[command(about = "Get block transaction count by number")]
    GetBlockTransactionCountByNumber {
        #[arg(short, long)]
        number: u64,
    },
    #[command(about = "Get block transaction count by hash")]
    GetBlockTransactionCountByHash {
        #[arg(short, long)]
        hash: String,
    },
    /// Returns a list of addresses owned by client
    Accounts,
    Explorer,
    Exit,
    /// Connects to a different EVM JSON-RPC server
    Connect { addr: String },
}

async fn run_explorer(client: &HttpClient) -> Result<(), Box<dyn std::error::Error>> {
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
                        let latest_hex: String = client.request("eth_blockNumber", rpc_params![]).await?;
                        let latest = u64::from_str_radix(latest_hex.trim_start_matches("0x"), 16)?;
                        println!("Latest blocks (showing up to 10):");
                        let start = if latest > 10 { latest - 9 } else { 0 };
                        for n in (start..=latest).rev() {
                            let block_id = BlockId::number(n);
                            let block: Option<Block> = client.request("eth_getBlockByNumber", rpc_params![block_id, false]).await?;
                            if let Some(b) = block {
                                println!("Block #{}: hash={}, txs={}", n, b.header.hash, b.transactions.len());
                            }
                        }
                    }
                    "block" => {
                        if parts.len() < 2 {
                            println!("Usage: block <number>");
                            continue;
                        }
                        if let Ok(number) = parts[1].parse::<u64>() {
                            let block_id = BlockId::number(number);
                            let block: Option<Block> = client.request("eth_getBlockByNumber", rpc_params![block_id, true]).await?;
                            if let Some(b) = block {
                                println!("Block Information:");
                                println!("  Number: {}", number);
                                println!("  Hash: {:?}", b.header.hash);
                                println!("  Parent Hash: {:?}", b.header.parent_hash);
                                println!("  Timestamp: {}", b.header.timestamp);
                                println!("  Transactions ({}):", b.transactions.len());
                            } else {
                                println!("Block not found");
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
                        let tx: Option<Transaction> = client.request("eth_getTransactionByHash", rpc_params![hash]).await?;
                        if let Some(t) = tx {
                            println!("Transaction Information:");
                            println!("  Hash: {}", t.inner.hash());
                            println!("  From: {}", t.inner.signer());
                            println!("  To: {:?}", t.inner.to());
                            println!("  Value: {} wei", t.inner.value());
                            println!("  Nonce: {}", t.inner.nonce());
                            println!("  Gas Limit: {}", t.inner.gas_limit());
                            println!("  Gas Price: {:?}", t.inner.gas_price());
                            println!("  Block: #{} ({:?})", t.block_number.unwrap_or_default(), t.block_hash.unwrap_or_default());
                        } else {
                            println!("Transaction not found");
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
    println!("Connecting to server at {}...", cli.server);
    let mut client = HttpClientBuilder::default().build(&cli.server)?;

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

                let args = match shlex::split(line) {
                    Some(args) => args,
                    None => {
                        println!("Invalid input");
                        continue;
                    }
                };

                let command_cli = match CommandCli::try_parse_from(args) {
                    Ok(cli) => cli,
                    Err(e) => {
                        println!("{}", e);
                        continue;
                    }
                };

                match command_cli.command {
                    Commands::BlockNumber => {
                        let res: String = client.request("eth_blockNumber", rpc_params![]).await?;
                        println!("Latest block number: {}", res);
                    }
                    Commands::GetBalance { address, block_tag } => {
                        let block_id = parse_block_id(&block_tag);
                        let res: String = client.request("eth_getBalance", rpc_params![address, block_id]).await?;
                        println!("Balance: {} wei", res);
                    }
                    Commands::GetTransactionCount { address, block_tag } => {
                        let block_id = parse_block_id(&block_tag);
                        let res: String = client.request("eth_getTransactionCount", rpc_params![address, block_id]).await?;
                        println!("Transaction count (nonce): {}", res);
                    }
                    Commands::GetBlockByNumber { number, full } => {
                        let block_id = BlockId::number(number);
                        let res: Option<Block> = client.request("eth_getBlockByNumber", rpc_params![block_id, full]).await?;
                        println!("{:#?}", res);
                    }
                    Commands::GetBlockByHash { hash, full } => {
                        let res: Option<Block> = client.request("eth_getBlockByHash", rpc_params![hash, full]).await?;
                        println!("{:#?}", res);
                    }
                    Commands::GetTransactionByHash { hash } => {
                        let res: Option<Transaction> = client.request("eth_getTransactionByHash", rpc_params![hash]).await?;
                        println!("{:#?}", res);
                    }
                    Commands::GetTransactionReceipt { hash } => {
                        let res: Option<TransactionReceipt> = client.request("eth_getTransactionReceipt", rpc_params![hash]).await?;
                        println!("{:#?}", res);
                    }
                    Commands::GetLogs { address, from_block, to_block, topics } => {
                        let mut filter = Filter::default();
                        if let Some(addr) = address {
                            filter = filter.address(Address::from_str(&addr)?);
                        }
                        if let Some(from) = from_block {
                            filter = filter.from_block(parse_block_tag(&from));
                        }
                        if let Some(to) = to_block {
                            filter = filter.to_block(parse_block_tag(&to));
                        }
                        for _topic in topics {
                            // topic setting is complex and depends on Filter version
                        }
                        let res: Vec<Log> = client.request("eth_getLogs", rpc_params![filter]).await?;
                        println!("{:#?}", res);
                    }
                    Commands::GetCode { address, block_tag } => {
                        let res: Bytes = client.request("eth_getCode", rpc_params![address, parse_block_id(&block_tag)]).await?;
                        println!("Code: {}", res);
                    }
                    Commands::GetStorageAt { address, slot, block_tag } => {
                        let res: String = client.request("eth_getStorageAt", rpc_params![address, slot, parse_block_id(&block_tag)]).await?;
                        println!("Storage value: {}", res);
                    }
                    Commands::ChainId => {
                        let res: String = client.request("eth_chainId", rpc_params![]).await?;
                        println!("Chain ID: {}", res);
                    }
                    Commands::GasPrice => {
                        let res: String = client.request("eth_gasPrice", rpc_params![]).await?;
                        println!("Gas price: {}", res);
                    }
                    Commands::GetBlockTransactionCountByNumber { number } => {
                        let res: Option<String> = client.request("eth_getBlockTransactionCountByNumber", rpc_params![BlockId::number(number)]).await?;
                        println!("Transaction count: {:?}", res);
                    }
                    Commands::GetBlockTransactionCountByHash { hash } => {
                        let res: Option<String> = client.request("eth_getBlockTransactionCountByHash", rpc_params![hash]).await?;
                        println!("Transaction count: {:?}", res);
                    }
                    Commands::Accounts => {
                        let res: Vec<String> = client.request("eth_accounts", rpc_params![]).await?;
                        println!("Accounts: {:?}", res);
                    }
                    Commands::Explorer => {
                        if let Err(e) = run_explorer(&client).await {
                            println!("Explorer error: {:?}", e);
                        }
                    }
                    Commands::Connect { addr } => {
                        println!("Connecting to server at {}...", addr);
                        client = HttpClientBuilder::default().build(&addr)?;
                    }
                    Commands::Exit => break,
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

fn parse_block_id(s: &str) -> BlockId {
    match s {
        "latest" => BlockId::latest(),
        "earliest" => BlockId::earliest(),
        "pending" => BlockId::pending(),
        "safe" => BlockId::safe(),
        "finalized" => BlockId::finalized(),
        _ => {
            if let Ok(num) = s.parse::<u64>() {
                BlockId::number(num)
            } else if let Ok(num) = u64::from_str_radix(s.trim_start_matches("0x"), 16) {
                BlockId::number(num)
            } else {
                BlockId::latest()
            }
        }
    }
}

fn parse_block_tag(s: &str) -> alloy_eips::BlockNumberOrTag {
    match s {
        "latest" => alloy_eips::BlockNumberOrTag::Latest,
        "earliest" => alloy_eips::BlockNumberOrTag::Earliest,
        "pending" => alloy_eips::BlockNumberOrTag::Pending,
        "safe" => alloy_eips::BlockNumberOrTag::Safe,
        "finalized" => alloy_eips::BlockNumberOrTag::Finalized,
        _ => {
            if let Ok(num) = s.parse::<u64>() {
                alloy_eips::BlockNumberOrTag::Number(num)
            } else if let Ok(num) = u64::from_str_radix(s.trim_start_matches("0x"), 16) {
                alloy_eips::BlockNumberOrTag::Number(num)
            } else {
                alloy_eips::BlockNumberOrTag::Latest
            }
        }
    }
}
