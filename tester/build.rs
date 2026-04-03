fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_client(true)
        .compile(&["../wasix-based-evm/proto/transaction.proto"], &["../wasix-based-evm/proto"])?;
    Ok(())
}