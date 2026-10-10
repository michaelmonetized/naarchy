# t3-live: T3 Code live activities

Shows active [T3 Code](https://github.com/pingdotgg/t3code) agent threads on the
Naarchy island: **project · status · thread title · elapsed**.

```
 naarchy · running · Fix pill width on notch displays · 12m  +2
```

| Thread state | Island | Priority |
|---|---|---|
| a request is waiting for you (`pendingRequestCount > 0`) | `needs you` | 70, outranks files and music |
| `preparing`, `queued`, `starting`, `running` | the status | 40 |
| `waiting` (turn over, background work draining) | `finishing` | 30 |

Threads leave the island when they complete, fail, are interrupted, or are settled.

## Install

Requires Python 3.8+ (standard library only) and a running T3 Code nightly
server (`t3 serve` or the `t3code` service).

```bash
naarchy plugin install contrib/plugins/t3-live
naarchy plugin run t3-live           # optional: see what the island will show
systemctl --user restart naarchy
```

## How it talks to T3 Code

Everything goes through T3 Code's MCP server: streamable HTTP at
`<origin>/mcp`, MCP protocol `2025-06-18`, bearer auth.

1. **Origin** comes from `~/.t3/userdata/server-runtime.json` (`origin`, usually
   `http://127.0.0.1:3773`), honoring `T3CODE_HOME`, or from `T3_URL`.
2. **Credential.** On first run the plugin asks the local CLI for a read-only
   MCP client session, once:

   ```bash
   t3 auth session issue --scope orchestration:read --subject mcp-client \
     --label "Naarchy live activities" --ttl 30d --token-only
   ```

   It is stored in `~/.local/share/naarchy/plugins/t3-live/mcp-token` (0600).
   When T3 rejects it (expired or revoked), the plugin mints a new one at most
   every 10 minutes. Revoke it any time with `t3 auth session list` /
   `t3 auth session revoke <id>`. To manage the token yourself, set
   `T3_MCP_TOKEN` (and `T3_LIVE_AUTO_TOKEN=0` to forbid minting).
3. **Tools** (all read-only): `t3_project_list`, then `t3_thread_list` per project
   with `statuses = [preparing, queued, starting, running, waiting]`, then
   `t3_thread_read` for each active thread (cached per run) to get the run's
   `startedAt` and `pendingRequestCount`. An outside MCP client has no calling
   thread, so `t3_thread_list` needs an explicit `projectId`.

Polling is every 4 seconds (`T3_LIVE_INTERVAL`). When the server is down the
island is cleared and the plugin keeps retrying quietly.

## Debug

```bash
T3_LIVE_ONCE=1 contrib/plugins/t3-live/bin/t3-live     # one JSON snapshot
python3 contrib/plugins/t3-live/test_t3_live.py        # offline test against a fake MCP server
```

## Limits

- Local environment only: the T3 server listens on loopback. Remote
  environments (other machines) would need their own credential and a tunnel.
- T3 has no MCP push notifications for thread status yet, so this polls.
