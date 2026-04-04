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
    /// Signs and submits a transaction
    SendTransaction {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "0")]
        value: String,
        #[arg(long, default_value = "")]
        data: String,
        #[arg(long, default_value = "21000")]
        gas_limit: u64,
        #[arg(long, default_value = "1000000000")]
        gas_price: u64,
        #[arg(long, default_value = "0")]
        nonce: u64,
    },
    /// Executes a transaction (similar to sendTransaction for now)
    ExecuteTransaction {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "0")]
        value: String,
        #[arg(long, default_value = "")]
        data: String,
        #[arg(long, default_value = "21000")]
        gas_limit: u64,
        #[arg(long, default_value = "1000000000")]
        gas_price: u64,
        #[arg(long, default_value = "0")]
        nonce: u64,
    },
    /// Exits the tester
    Exit,
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