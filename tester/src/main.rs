pub mod evm_rpc {
    tonic::include_proto!("evm_rpc");
}

use evm_rpc::transaction_service_client::TransactionServiceClient;
use evm_rpc::TransactionRequest;
use alloy_primitives::{address, uint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TransactionServiceClient::connect("http://127.0.0.1:50051").await?;

    println!("Connected to EVM gRPC Server");

    let from_addr = address!("0101010101010101010101010101010101010101");
    let to_addr = address!("0202020202020202020202020202020202020202");
    let _transfer_value = uint!(1000000000000000_U256); // 0.001 ETH

    let request = tonic::Request::new(TransactionRequest {
        from: from_addr.to_string(),
        to: to_addr.to_string(),
        value: 1000000000000000u64, // simplified for demo
        data: vec![],
        gas_limit: 100000,
        gas_price: 1000000000,
        nonce: 0,
    });

    println!("Sending transaction request...");
    let response = client.execute_transaction(request).await?;

    println!("RESPONSE: {:?}", response.into_inner());

    println!("Querying accounts...");
    let accounts = client.eth_accounts(tonic::Request::new(evm_rpc::Empty {})).await?;
    println!("ACCOUNTS: {:?}", accounts.into_inner().addresses);

    println!("Querying latest block number...");
    let block_number = client.eth_block_number(tonic::Request::new(evm_rpc::Empty {})).await?;
    println!("BLOCK NUMBER: {:?}", block_number.into_inner().number);

    Ok(())
}