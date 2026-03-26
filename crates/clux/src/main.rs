// Use Windows subsystem to prevent a console window from appearing.
// The terminal runs inside ConPTY, not in the host console.
#![windows_subsystem = "windows"]

mod app;
pub mod config;
pub mod selection;

use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    // Log to file since windows_subsystem="windows" has no stderr
    let log_file = std::fs::File::create(
        dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("clux")
            .join("clux.log"),
    )
    .ok();

    if let Some(file) = log_file {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("info,clux_terminal::vt_parser=debug"))
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    let config = config::load_config();
    app::run(config)
}
