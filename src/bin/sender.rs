use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use screen_mirror::ui::app;
use screen_mirror::ui::backend;
use screen_mirror::ui::messages::*;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let (cmd_tx, cmd_rx) = mpsc::channel::<UiCommand>();
    let (event_tx, event_rx) = mpsc::channel::<BackendEvent>();

    let shared_devices = Arc::new(Mutex::new(Vec::new()));

    // Spawn mDNS browser background thread
    let _mdns_handle = backend::spawn_mdns_browser(event_tx.clone(), shared_devices.clone());

    // Spawn command handler background thread
    let _cmd_handle = backend::spawn_command_handler(cmd_rx, event_tx, shared_devices);

    // Run egui tray app on main thread (blocks until exit)
    app::run(cmd_tx, event_rx).map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}
