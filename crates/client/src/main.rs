use fluxion_client::cli;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    dotenvy::dotenv().ok();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (_, handled) = cli::dispatch(args).await;
    if let Some(code) = handled {
        std::process::exit(code);
    }
    eprintln!("serve is provided by the fluxion-remote binary");
    std::process::exit(2);
}
