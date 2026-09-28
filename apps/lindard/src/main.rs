use lindar_core::{NodeId, NodeInfo, OperatingSystem};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_owned());
    let node = NodeInfo {
        id: NodeId::new(hostname.clone()),
        hostname,
        operating_system: OperatingSystem::current(),
    };

    info!(
        node_id = %node.id,
        hostname = %node.hostname,
        operating_system = ?node.operating_system,
        "Lindar node started"
    );
    println!("Lindar node: {} ({:?})", node.hostname, node.operating_system);

    tokio::signal::ctrl_c().await?;
    info!("shutdown signal received");
    Ok(())
}

