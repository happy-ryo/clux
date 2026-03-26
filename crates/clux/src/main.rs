// Use Windows subsystem to prevent a console window from appearing.
// The terminal runs inside ConPTY, not in the host console.
#![windows_subsystem = "windows"]

mod app;
pub mod config;
pub mod selection;

use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config::load_config();
    app::run(config)
}
