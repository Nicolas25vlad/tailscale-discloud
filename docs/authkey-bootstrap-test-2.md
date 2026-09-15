# Auth-key bootstrap regression test

Observed on Discloud with a fresh container:

- `TAILSCALE_AUTHKEY` is present.
- `tailscale status` reports `Logged out` after startup.
- Running `tailscale up --authkey="$TAILSCALE_AUTHKEY" --hostname="$TAILSCALE_HOSTNAME" --accept-routes --accept-dns` manually authenticates successfully.

This fix therefore treats the Tailscale CLI flow as the compatibility path for fresh-node auth-key bootstrap, while keeping LocalAPI for restored sessions and interactive login.
