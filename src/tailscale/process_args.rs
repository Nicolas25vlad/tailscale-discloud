pub fn build_tailscaled_args(
    state_path: &str,
    socket_path: &str,
    socks5_server: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "--tun=userspace-networking".to_string(),
        format!("--state={state_path}"),
        format!("--socket={socket_path}"),
    ];

    if let Some(listen_addr) = socks5_server
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        args.push(format!("--socks5-server={listen_addr}"));
    }

    args
}

#[cfg(test)]
mod tests {
    use super::build_tailscaled_args;

    #[test]
    fn builds_userspace_args_without_proxy_by_default() {
        let args = build_tailscaled_args(
            "/home/discloud/tailscale.state",
            "/var/run/tailscale/tailscaled.sock",
            None,
        );

        assert_eq!(
            args,
            vec![
                "--tun=userspace-networking",
                "--state=/home/discloud/tailscale.state",
                "--socket=/var/run/tailscale/tailscaled.sock",
            ]
        );
    }

    #[test]
    fn appends_socks5_listener_when_configured() {
        let args = build_tailscaled_args(
            "/home/discloud/tailscale.state",
            "/var/run/tailscale/tailscaled.sock",
            Some("0.0.0.0:1055"),
        );

        assert_eq!(
            args,
            vec![
                "--tun=userspace-networking",
                "--state=/home/discloud/tailscale.state",
                "--socket=/var/run/tailscale/tailscaled.sock",
                "--socks5-server=0.0.0.0:1055",
            ]
        );
    }

    #[test]
    fn ignores_blank_socks5_listener() {
        let args = build_tailscaled_args(
            "/home/discloud/tailscale.state",
            "/var/run/tailscale/tailscaled.sock",
            Some("   "),
        );

        assert_eq!(
            args,
            vec![
                "--tun=userspace-networking",
                "--state=/home/discloud/tailscale.state",
                "--socket=/var/run/tailscale/tailscaled.sock",
            ]
        );
    }
}
