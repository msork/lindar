# Lindar architecture

Lindar is a Rust workspace for building a shared desktop experience between
Linux and macOS. The initial foundation defines platform-neutral node and
display types, a serializable peer message, a networking boundary, and a small
daemon entry point. It does not yet capture input or screens and contains no
GUI, video, Wayland, or macOS integration.

## Workspace crates

- `lindar-core` owns shared domain types such as `NodeId`, `NodeInfo`,
  `OperatingSystem`, `DisplayInfo`, and `DisplayGeometry`. It has no operating
  system API dependencies.
- `lindar-protocol` owns serde-based messages exchanged between peers and
  depends on the core types.
- `lindar-network` is the boundary for future transport and peer communication.
  It depends on the protocol and core crates, but has no transport behavior yet.
- `lindard` is the daemon executable. It reports its node identity, initializes
  tracing, and waits for Ctrl+C.

Tokio supplies the daemon's async runtime and signal handling. `tracing` is used
for structured logs, and `thiserror` is available to the networking layer for
typed errors as transport behavior is added.

## Dependency direction

```text
lindard -> lindar-network -> lindar-protocol -> lindar-core
    |               `------------------------------->|
    `----------------------------------------------->|
```

Platform integrations can be added behind the daemon and networking boundaries
later without putting platform-specific APIs into the shared core model.

