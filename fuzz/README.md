# Fuzzing

cargo-fuzz targets for every place that parses input from the modem, a kernel
interface or another process. They need a nightly toolchain and
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz):

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run imeiprov_message fuzz/corpus/imeiprov_message fuzz/seeds/imeiprov_message
```

| Target | Input | Code |
| --- | --- | --- |
| `imeiprov_message` | modem: TCL QMI NV read and write replies | `imeiprov/src/message.rs` |
| `imeiprov_trace` | kernel: traceability partition windows (IMEI, Bluetooth address, Wi-Fi MAC), NV 550 values | `imeiprov/src/{imei,bdaddr,wlanmac,eui48}.rs` |
| `imeiprov_provision` | modem: NV read results and write acknowledgements during `--write` | `imeiprov/src/provision.rs` |
| `dcm_frame` | modem: QMI framing and DCM request decoding | `dcm/src/protocol.rs` |
| `dcm_qrtr_control` | kernel: QRTR name service control packets | `dcm/src/qrtr.rs` |
| `dcm_engine` | modem datagrams, broker reports, client and node loss, deferred output | `dcm/src/engine.rs`, `dcm/src/outgoing.rs` |
| `daemon_outbox` | send results and timing for queued modem output | `daemon/src/outbox.rs` |
| `daemon_broker_report` | broker Binder reports | `daemon/src/broker_report.rs` |
| `wlan_codec` | modem: DSD responses, keepalive instructions, profile notices | `wlan/src/{lib,profile_notice,profiles}.rs` |
| `wlan_session` | modem packets, observer snapshots and the clock for one DSD session | `wlan/src/{session,keepalive,profile_reporting}.rs` |
| `wlan_runtime_observations` | Wi-Fi observer Binder snapshots, registration and death | `wlan-runtime/src/lib.rs` |

The Java IMS broker has a random-input driver instead:
`tests/fuzz/run-broker-fuzz.sh` (see the script).

Seeds in `seeds/` are built from made-up values only (all-zero TAC IMEIs as in
`imeiprov/tests`, documentation addresses). Never add a real IMEI, address or
other identifier, and keep generated corpora (`corpus/`, ignored) out of the
repository. No target prints decoded identifiers.

Not reached by these targets: the socket and syscall wrappers themselves
(`daemon/src/socket.rs`, `imeiprov/src/client.rs`, `trace.rs` file reads) and
the Binder glue in `daemon/src/main.rs` and `wlan-runtime/src/main.rs`, which
need Android's Binder library. The checks those call are fuzzed above.
