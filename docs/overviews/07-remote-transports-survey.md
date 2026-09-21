# Remote transports: Timely and Arroyo field survey

> **Last refreshed:** 2026-09-14

> **Scope:** how Timely Dataflow and Arroyo implement their cross-worker TCP data plane, and
> what that means for Malstrom's future remote transport. Facts below were verified from
> `timely_communication` and `arroyo-worker` source on 2026-09-14. The design proposal that
> borrows their shapes is
> [`.agents/notes/proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md`](../../.agents/notes/proposed/architecture/2026-09-13-unify-operator-io-edge-abstractions.md).

## Both write their own TCP transport

Neither Timely nor Arroyo uses gRPC, HTTP, QUIC, or an off-the-shelf streaming framework for
the data plane. Both hand-roll the wire protocol and connection lifecycle on top of OS TCP
sockets — "their own TCP transport" means *their own framing and connection management*, not
implementing TCP itself.

| Aspect | Timely Dataflow | Arroyo |
|---|---|---|
| TCP layer | `std::net::{TcpListener, TcpStream}` (blocking) | `tokio::net::{TcpListener, TcpStream}` (async) |
| Framing | custom `MessageHeader` (6 `usize` fields, ~48 bytes on 64-bit) | custom `Header` (`src_operator`, `src_subtask`, `dst_operator`, `dst_subtask`, `len`, `type`) |
| Connection topology | one TCP connection per worker pair, channels multiplexed | one outbound `OutNetworkLink` per `Quad` (logical subtask→subtask edge) |
| Receive demux | framed headers route messages to the right channel | `InNetworkLink` reads headers and dispatches to `HashMap<Quad, NetworkSender>` |
| TLS | none built-in | optional `tokio-rustls` (`NetworkStream::Tls*`) |
| Runtime | dedicated threads, blocking I/O | tokio tasks, async I/O |

Sources:

- Timely — [`communication/src/networking.rs`](https://github.com/TimelyDataflow/timely-dataflow/blob/master/communication/src/networking.rs)
- Arroyo — [`crates/arroyo-worker/src/network_manager.rs`](https://github.com/ArroyoSystems/arroyo/blob/master/crates/arroyo-worker/src/network_manager.rs)

## Timely specifics

- `communication/src/networking.rs` handles peer bootstrap with `create_sockets` /
  `start_connections` / `await_connections`: each worker binds listeners and connects
  `TcpStream`s to the other addresses in the cluster.
- The custom `MessageHeader` carries the channel index plus a few `usize` fields and is
  encoded/decoded by hand (`try_read`, `write`), then payloads flow over the connection.
- Send/recv run on dedicated threads with blocking I/O.

## Arroyo specifics

- `crates/arroyo-worker/src/network_manager.rs` owns both sides: `OutNetworkLink::connect`
  (with retries and optional TLS via `tokio-rustls`) and `InNetworkLink` (buffered reader over
  `NetworkStream`).
- A `Quad` — source operator id, source subtask index, destination operator id, destination
  subtask index — is the map key for every logical edge in `Senders` and `out_streams`. The
  wire `Header` is that quad plus length and message type, so inbound data can be
  demultiplexed back to the right `Quad`.
- Because each `Quad` gets its own `OutNetworkLink`, Arroyo effectively opens one TCP
  connection per logical subtask-to-subtask edge.

## Why it matters for Malstrom

- The unification proposal explicitly **borrows shapes, not code** from Timely and Arroyo.
  Since both transports are hand-written, there is no off-the-shelf transport to adopt either;
  a future Malstrom TCP relay is also a custom implementation.
- The note recommends **Timely-style worker-pair multiplexing** (one connection per pair,
  headers demux channels) over Arroyo's per-`Quad` connections, to avoid socket explosion on
  wide graphs.
- Malstrom's remote path is already trait-based (`OperatorOperatorComm` /
  `WorkerCoordinatorComm`): the threaded flavor uses `flume`, the k8s flavor uses gRPC, and a
  future custom TCP relay would implement the same traits without touching operator code.