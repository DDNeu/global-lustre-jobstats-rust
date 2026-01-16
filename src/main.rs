//! glljobstat - Global Lustre Job Statistics Tool
//!
//! Read job_stats files, parse and aggregate data of every job on multiple
//! OSS/MDS via SSH using key or password, show top jobs and more.

mod args;
mod config;
mod error;
mod job_stats;
mod logging;
mod op_keys;
mod output;
mod persistence;
mod ssh;
mod stats_processor;
mod tui;

use anyhow::{Context, Result};
use std::fs;
use std::process;

use args::Args;
use config::{default_config_template, Config, ConfigFile};
use job_stats::JobStatsParser;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Parse args and track which were explicitly provided
    let (mut args, provided) = Args::parse_with_provided()?;

    // Load config file to check for profiles
    let config_path = expand_tilde(&args.configfile);

    // Handle --init: generate example config file
    if args.init {
        if config_path.exists() {
            anyhow::bail!(
                "Config file already exists: {}\nUse a different path with -C to create a new config file,\nor delete the existing file first.",
                config_path.display()
            );
        }

        let content = default_config_template();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&config_path, &content).context("Failed to write config file")?;
        println!("Example configuration file created: {}", config_path.display());
        println!("\nEdit the file and uncomment a profile to get started.");
        println!("Then run: glljobstat -P <profile_name>");
        return Ok(());
    }

    let config_file = if config_path.exists() {
        let content = fs::read_to_string(&config_path).context("Failed to read config file")?;
        toml::from_str::<ConfigFile>(&content).context("Failed to parse config file")?
    } else {
        ConfigFile::default()
    };

    // Handle --list-profiles
    if args.list_profiles {
        let profiles = config_file.list_profiles();
        if profiles.is_empty() {
            println!("No profiles defined in {}", config_path.display());
            println!("\nTo create a profile, add a [profile.NAME] section to the config file.");
            println!("Example:\n");
            println!("[profile.tui-monitor]");
            println!("tui = \"true\"");
            println!("rate = true");
            println!("interval = 5");
            println!("count = 20");
        } else {
            println!("Available profiles in {}:", config_path.display());
            for name in profiles {
                println!("  - {}", name);
            }
        }
        return Ok(());
    }

    // Apply profile if specified and get reference for SSH settings
    let active_profile = if let Some(ref profile_name) = args.profile {
        if let Some(profile) = config_file.get_profile(profile_name) {
            args.apply_profile(profile, &provided);
            Some(profile)
        } else {
            let available: Vec<_> = config_file.list_profiles();
            if available.is_empty() {
                anyhow::bail!(
                    "Profile '{}' not found. No profiles are defined in {}",
                    profile_name,
                    config_path.display()
                );
            } else {
                anyhow::bail!(
                    "Profile '{}' not found. Available profiles: {}",
                    profile_name,
                    available
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    } else {
        None
    };

    // Apply post-processing logic (OST/MDT shortcuts, totalrate implications, etc.)
    args.finalize();

    // Now load the full runtime config, passing profile for SSH settings
    let config = Config::load_or_create(&args, active_profile)?;

    // Check if TUI mode is requested
    if let Some(ref replay_path) = args.tui {
        // Create parser for TUI
        let parser = JobStatsParser::new(args.clone(), config.clone());

        // Determine if this is replay mode (path provided) or live mode (empty string)
        let replay_path = if replay_path.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(replay_path))
        };

        return tui::run(config, parser, replay_path).await;
    }

    // Normal CLI mode
    let mut parser = JobStatsParser::new(args, config);
    parser.run().await?;

    Ok(())
}

/// Expand ~ to home directory
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    std::path::PathBuf::from(path)
}

