# Lindar architecture

Lindar is a Rust workspace for building a shared desktop experience between
Linux and macOS. The foundation defines platform-neutral node and display
types, a serializable peer message, LAN peer discovery, and a small daemon entry
point. It does not capture input or screens and contains no GUI, video,
Wayland, or macOS API integration.

## Workspace crates

- `lindar-core` owns shared domain types such as `NodeId`, `NodeInfo`,
  `OperatingSystem`, `DisplayInfo`, and `DisplayGeometry`. It has no operating
  system API dependencies.
- `lindar-protocol` owns serde-based messages exchanged between peers and
  depends on the core types.
- `lindar-network` advertises Lindar nodes and browses `_lindar._tcp.local.`
  with mDNS. It validates required TXT metadata and protocol compatibility,
  filters its own node ID, and maintains an in-memory table with discovered,
  updated, and lost events. mDNS expiry and goodbye records remove peers.
- `lindard` is the daemon executable. Before discovery, it detects the system
  hostname and loads or creates a persistent UUID v4 node ID. Linux stores the
  ID under `$XDG_STATE_HOME/lindar` (or `~/.local/state/lindar`); macOS stores it
  under `~/Library/Application Support/Lindar`. A malformed stored identity is
  reported as an error instead of being silently replaced. The daemon then
  initializes tracing, starts discovery, logs peer lifecycle changes, and
  waits for Ctrl+C.

`gethostname` retrieves the host name across Linux and macOS, and `uuid` creates
random version 4 node identifiers. `mdns-sd` provides DNS-SD advertisement and
browsing. Tokio supplies async event handling and daemon signal handling.
`tracing` is used for structured logs, and `thiserror` provides typed errors.

## Dependency direction

```text
lindard -> lindar-network -> lindar-protocol -> lindar-core
    |               `------------------------------->|
    `----------------------------------------------->|
```

Platform integrations can be added behind the daemon and networking boundaries
later without putting platform-specific APIs into the shared core model.
