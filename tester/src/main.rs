pub mod evm_rpc {
    tonic::include_proto!("evm_rpc");
}

use clap::{Parser, Subcommand};
use evm_rpc::transaction_service_client::TransactionServiceClient;
use evm_rpc::*;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

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
    /// Opens the block explorer
    Explorer,
    /// Exits the tester
    Exit,
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
                            to,
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
                            to,
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
                            to,
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