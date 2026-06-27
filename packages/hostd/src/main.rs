use hostd::run_stdio_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    run_stdio_server().await?;
    Ok(())
}
