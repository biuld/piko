use piko_hostd::{
    logging::{init, parse_hostd_log_cli, resolve_config},
    run_stdio_server,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_hostd_log_cli(std::env::args().skip(1));
    let config = resolve_config(&cli)?;
    let _log_guard = init(config)?;

    run_stdio_server().await?;
    Ok(())
}
