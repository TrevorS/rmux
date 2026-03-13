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

    // Parse arguments: rmux-server [socket-path] [-f config-file]
    let args: Vec<String> = std::env::args().collect();
    let mut socket_path = None;
    let mut config_file = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "-f" {
            i += 1;
            if i < args.len() {
                config_file = Some(args[i].clone());
            }
        } else if socket_path.is_none() {
            socket_path = Some(std::path::PathBuf::from(&args[i]));
        }
        i += 1;
    }

    let socket_path = socket_path.unwrap_or_else(Server::default_socket_path);

    tracing::info!(
        "rmux server {} starting (protocol v{})",
        env!("CARGO_PKG_VERSION"),
        rmux_protocol::message::PROTOCOL_VERSION,
    );

    let mut server = Server::new(socket_path);
    if let Err(e) = server.run(config_file.as_deref()).await {
        tracing::error!("server error: {e}");
        std::process::exit(1);
    }
}
