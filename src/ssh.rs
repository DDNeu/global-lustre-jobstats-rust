//! SSH connectivity for glljobstat

use anyhow::{Context, Result};
use ssh2::Session;
use std::io::Read;
use std::net::TcpStream;
use std::path::Path;

use crate::config::{Config, ServerCredentials};

/// SSH connection wrapper
pub struct SshConnection {
    session: Session,
}

impl SshConnection {
    /// Create a new SSH connection using per-server credentials
    pub fn connect_with_credentials(creds: &ServerCredentials) -> Result<Self> {
        let addr = if creds.host.contains(':') {
            creds.host.clone()
        } else {
            format!("{}:22", creds.host)
        };

        let tcp = TcpStream::connect(&addr)
            .with_context(|| format!("Failed to connect to {}", addr))?;

        let mut session = Session::new().context("Failed to create SSH session")?;

        session.set_tcp_stream(tcp);
        session.handshake().context("SSH handshake failed")?;

        // Authenticate
        if let Some(ref key_path) = creds.key {
            // Key-based authentication
            let key_path = Path::new(key_path);
            session
                .userauth_pubkey_file(&creds.user, None, key_path, None)
                .with_context(|| {
                    format!(
                        "SSH key authentication failed for user {} on {}",
                        creds.user, creds.host
                    )
                })?;
        } else if let Some(ref password) = creds.password {
            // Password authentication
            session
                .userauth_password(&creds.user, password)
                .with_context(|| {
                    format!(
                        "SSH password authentication failed for user {} on {}",
                        creds.user, creds.host
                    )
                })?;
        } else {
            anyhow::bail!(
                "No SSH authentication method available for {} (no key or password)",
                creds.host
            );
        }

        if !session.authenticated() {
            anyhow::bail!("SSH authentication failed for {}", creds.host);
        }

        Ok(SshConnection { session })
    }

    /// Create a new SSH connection to the given host (looks up credentials from config)
    pub fn connect(host: &str, config: &Config) -> Result<Self> {
        let creds = config.get_credentials(host);
        Self::connect_with_credentials(&creds)
    }

    /// Execute a command and return the output
    pub fn exec(&self, command: &str) -> Result<String> {
        let mut channel = self
            .session
            .channel_session()
            .context("Failed to open SSH channel")?;

        channel
            .exec(command)
            .with_context(|| format!("Failed to execute command: {}", command))?;

        let mut output = String::new();
        channel
            .read_to_string(&mut output)
            .context("Failed to read command output")?;

        channel.wait_close().ok(); // Ignore close errors

        Ok(output)
    }
}

/// Query type for SSH operations
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum QueryType {
    Param,
    Stats,
}

/// Result of SSH query for parameters
#[derive(Debug, Clone)]
pub struct ParamResult {
    pub host: String,
    pub params: Vec<String>,
}

/// Result of SSH query for stats
#[derive(Debug, Clone)]
pub struct StatsResult {
    #[allow(dead_code)]
    pub host: String,
    pub data: String,
}

/// Execute SSH command to get parameters from a host
pub fn get_params(host: &str, param_pattern: &str, config: &Config) -> Result<ParamResult> {
    let conn = SshConnection::connect(host, config)?;
    let cmd = format!("lctl list_param {}", param_pattern);
    let output = conn.exec(&cmd)?;

    let params: Vec<String> = output
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(ParamResult {
        host: host.to_string(),
        params,
    })
}

/// Execute SSH command to get job stats from a host
pub fn get_stats(host: &str, param: &str, config: &Config) -> Result<StatsResult> {
    let conn = SshConnection::connect(host, config)?;
    let cmd = format!("lctl get_param -n {}", param);
    let output = conn.exec(&cmd)?;

    Ok(StatsResult {
        host: host.to_string(),
        data: output,
    })
}

/// Get jobid_name from a host
pub fn get_jobid_name(host: &str, config: &Config) -> Result<String> {
    let conn = SshConnection::connect(host, config)?;
    let output = conn.exec("lctl get_param -n jobid_name")?;
    Ok(output.trim().to_string())
}

