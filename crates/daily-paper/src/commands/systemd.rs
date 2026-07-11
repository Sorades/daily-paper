use std::path::Path;

use anyhow::{Context, Result};
use directories::BaseDirs;
use tracing::info;

const SERVICE_NAME: &str = "daily-paper.service";

fn user_config_dir() -> Result<std::path::PathBuf> {
    BaseDirs::new()
        .map(|b| b.config_dir().to_path_buf())
        .context("failed to determine user config directory")
}

/// Install user-level systemd service.
pub fn install(data_dir: &Path) -> Result<()> {
    let binary = std::env::current_exe().context("failed to determine current executable path")?;

    let service_dir = user_config_dir()?.join("systemd").join("user");

    std::fs::create_dir_all(&service_dir)
        .with_context(|| format!("failed to create {}", service_dir.display()))?;

    let service_path = service_dir.join(SERVICE_NAME);

    let content = format!(
        r#"[Unit]
Description=Daily Paper - Paper recommendation and deep reading pipeline
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={} --directory {}
Restart=on-failure
RestartSec=30
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
"#,
        binary.display(),
        data_dir.display(),
    );

    std::fs::write(&service_path, content)
        .with_context(|| format!("failed to write {}", service_path.display()))?;

    // Reload systemd daemon
    std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .context("failed to run systemctl --user daemon-reload")?;

    info!("service file installed to {}", service_path.display());
    println!("✓ Installed {}", service_path.display());
    println!();
    println!("To enable and start:");
    println!("  systemctl --user enable --now daily-paper");
    println!();
    println!("To view logs:");
    println!("  journalctl --user -u daily-paper -f");

    Ok(())
}

/// Remove systemd service.
pub fn uninstall(_data_dir: &Path) -> Result<()> {
    let service_path = user_config_dir()?
        .join("systemd")
        .join("user")
        .join(SERVICE_NAME);

    if !service_path.exists() {
        println!("Service file not found at {}", service_path.display());
        return Ok(());
    }

    // Try to stop and disable, but don't fail if not running
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", "daily-paper"])
        .status();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "daily-paper"])
        .status();

    std::fs::remove_file(&service_path)
        .with_context(|| format!("failed to remove {}", service_path.display()))?;

    // Reload systemd daemon
    std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .context("failed to run systemctl --user daemon-reload")?;

    println!("✓ Removed {}", service_path.display());
    Ok(())
}
