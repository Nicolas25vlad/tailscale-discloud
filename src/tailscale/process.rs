use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::tailscale::localapi::endpoints::login_interactive;
use crate::tailscale::localapi::{get_local_status, get_prefs, start, Options, Prefs};
use crate::tailscale::login_flow::{choose_login_method, LoginMethod};
use crate::tailscale::process_args::build_tailscaled_args;

const TAILSCALE_SOCKET_PATH: &str = "/var/run/tailscale/tailscaled.sock";

/// Spawn the `tailscaled` daemon as a child process.
pub async fn start_tailscaled() -> Result<tokio::process::Child, String> {
    let tailscale_state = std::env::var("TAILSCALE_STATE")
        .unwrap_or_else(|_| "/home/discloud/tailscale.state".to_string());
    let socks5_server = std::env::var("TAILSCALE_SOCKS5_LISTEN")
        .ok()
        .filter(|value| !value.trim().is_empty());

    tracing::debug!("Starting tailscaled child process");
    if let Some(listen_addr) = socks5_server.as_deref() {
        tracing::info!(listen_addr = %listen_addr, "Outbound SOCKS5 proxy enabled");
    }

    let args = build_tailscaled_args(
        &tailscale_state,
        TAILSCALE_SOCKET_PATH,
        socks5_server.as_deref(),
    );

    let mut cmd = Command::new("tailscaled");
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn tailscaled: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to open tailscaled stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to open tailscaled stderr")?;

    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            tracing::info!(target: "tailscaled", "{}", line);
        }
    });

    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            tracing::info!(target: "tailscaled", "{}", line);
        }
    });

    Ok(child)
}

/// Authenticate a fresh node with the same CLI path that `tailscale up` uses.
///
/// The auth key is placed in a temporary mode-0600 file and referenced with
/// `file:` so the secret is not exposed in the process argument list.
async fn authenticate_with_auth_key(hostname: &str, auth_key: &str) -> Result<(), String> {
    let auth_file = format!("/tmp/tailscale-authkey-{}", std::process::id());

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&auth_file)
        .map_err(|e| format!("Failed to create temporary Tailscale auth-key file: {e}"))?;

    file.write_all(auth_key.as_bytes())
        .map_err(|e| format!("Failed to write temporary Tailscale auth-key file: {e}"))?;
    drop(file);

    tracing::info!("Authenticating Tailscale with configured auth key");

    let output = Command::new("tailscale")
        .arg("up")
        .arg(format!("--auth-key=file:{auth_file}"))
        .arg(format!("--hostname={hostname}"))
        .arg("--accept-routes")
        .arg("--accept-dns")
        .output()
        .await;

    if let Err(e) = std::fs::remove_file(&auth_file) {
        tracing::warn!(error = %e, "Failed to remove temporary Tailscale auth-key file");
    }

    let output = output.map_err(|e| format!("Failed to run `tailscale up`: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(format!("`tailscale up` authentication failed: {detail}"));
    }

    Ok(())
}

/// Tailscale initialization flow:
/// 1. Spawn tailscaled.
/// 2. Wait for the LocalAPI socket to become available.
/// 3. If the node already has a node key, restore its prefs/session.
/// 4. If a fresh node has TAILSCALE_AUTHKEY, authenticate through `tailscale up`.
/// 5. Otherwise fall back to the interactive LocalAPI login flow.
/// 6. Poll status until authentication completes.
pub async fn init_tailscale_flow() -> Result<tokio::process::Child, String> {
    let tailscaled_child = start_tailscaled().await?;

    let mut waiting_socket = false;
    let initial_status = loop {
        match get_local_status().await {
            Ok(status) => break status,
            Err(_) => {
                if !waiting_socket {
                    tracing::debug!("Waiting for tailscaled socket to be ready");
                    waiting_socket = true;
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    };

    let mut last_auth_url: Option<String> = initial_status.auth_url.filter(|u| !u.is_empty());
    let hostname =
        std::env::var("TAILSCALE_HOSTNAME").unwrap_or_else(|_| "tailscale-discloud".to_string());
    let auth_key = std::env::var("TAILSCALE_AUTHKEY")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let login_method = choose_login_method(initial_status.have_node_key, auth_key.is_some());

    match login_method {
        LoginMethod::AuthKey => {
            let auth_key = auth_key
                .as_deref()
                .ok_or("Auth-key bootstrap selected without TAILSCALE_AUTHKEY")?;
            authenticate_with_auth_key(&hostname, auth_key).await?;
        }
        LoginMethod::None | LoginMethod::Interactive => {
            let mut prefs = match get_prefs().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "Failed to query current prefs from tailscaled: {}; using default prefs",
                        e
                    );
                    Prefs::default()
                }
            };

            prefs.hostname = Some(hostname);
            prefs.route_all = Some(true);
            prefs.corp_dns = Some(true);
            prefs.want_running = Some(true);

            let opts = Options {
                frontend_log_id: None,
                update_prefs: Some(prefs),
                auth_key: None,
            };

            tracing::debug!("Starting and configuring Tailscale via LocalAPI");
            start(opts).await?;

            if login_method == LoginMethod::Interactive {
                tracing::debug!("Starting interactive Tailscale login phase...");
                login_interactive().await?;
            }
        }
    }

    let mut last_state: Option<String> = None;

    loop {
        match get_local_status().await {
            Ok(status) => {
                tracing::trace!("Tailscale status: {:#?}", &status);
                let state = status.backend_state;
                let state_changed = Some(&state) != last_state.as_ref();
                if state_changed {
                    tracing::trace!("Tailscale state transition: {:?}", state);
                    last_state = Some(state.clone());
                }

                if state == "Running" {
                    tracing::info!("Tailscale is authenticated and running.");
                    break;
                } else if state == "NeedsLogin" && login_method == LoginMethod::Interactive {
                    if let Some(auth_url) = status.auth_url {
                        if Some(&auth_url) != last_auth_url.as_ref() && !auth_url.is_empty() {
                            let clickable = crate::logging::clickable_terminal_link(&auth_url);
                            crate::logging::print_box(&[
                                "To authenticate, visit:".to_string(),
                                format!("  {clickable}"),
                            ]);
                            last_auth_url = Some(auth_url);
                        }
                    }
                } else if state == "NeedsMachineAuth" && state_changed {
                    tracing::warn!("Machine requires authorization from the Tailnet administrator.")
                }
            }
            Err(e) => {
                tracing::error!("Failed to query status: {}", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    Ok(tailscaled_child)
}
