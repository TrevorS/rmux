//! rmux server entry point.

use rmux_server::server::Server;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rmux=info".parse().unwrap()),
        )
        .init();

    // Determine socket path
    let socket_path = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(Server::default_socket_path);

    tracing::info!(
        "rmux server {} starting (protocol v{})",
        env!("CARGO_PKG_VERSION"),
        rmux_protocol::message::PROTOCOL_VERSION,
    );

    let mut server = Server::new(socket_path);
    if let Err(e) = server.run().await {
        tracing::error!("server error: {e}");
        std::process::exit(1);
    }
}
