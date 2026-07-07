//! landingpig CLI — terminal AI agent for landing page engineering.

mod api;
mod app;
mod config;
mod fs;
mod state;
mod ui;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start tokio runtime")?;

    let mut app = app::App::new()?;
    app.run_ui(&rt)?;
    Ok(())
}
