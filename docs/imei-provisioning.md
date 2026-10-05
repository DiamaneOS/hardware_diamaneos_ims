# IMEI provisioning (imeiprovd)

`imeiprovd` is a minimal, source-built, one-shot tool that gives the modem its
IMEIs on DiamaneOS. DiamaneOS does not ship Fairphone's closed traceability
daemon, so the modem otherwise reports Qualcomm's placeholder IMEI on both
slots. `imeiprovd` reproduces only the modem-NV part of that daemon: it does not
set properties, write Bluetooth/Wi-Fi MAC files, or touch the region code.

## What it does

1. Opens the `traceability` partition **read-only** and reads just the two IMEI
   fields (15 ASCII digits each).
2. Decodes and validates both: exactly 15 digits, a correct Luhn check digit,
   and the two must differ. The modem's unprovisioned placeholder value is not
   Luhn-valid, so the Luhn test also rejects it; no placeholder constant is
   embedded in the source.
3. Reads the modem's current NV IMEI (item 550) over the Fairphone TCL QMI
   service on QRTR and compares it with the traceability values.
4. In `--write` mode only, writes the NV item for each subscription whose value
   differs, then reads it back to verify.

It logs only booleans and counters (logcat tag `imeiprovd`); it never logs an
IMEI or any other identifier.

## Modes

- `--check` (the shipped configuration): decode, validate and compare; never
  writes. The init service (`imeiprovd.rc`) runs `imeiprovd --check`.
- `--write`: as `--check`, then provision the differing NV items and verify.

Writing is enabled by changing the single argument in `imeiprovd.rc` from
`--check` to `--write`. `--write` fails closed: if validation fails, or the
modem rejects the write, or the read-back does not match, it writes nothing (or
reports failure) rather than leaving a bad value.

## Privileges

- Dedicated vendor user/group `vendor_imeiprov` (UID/GID 2994, `config.fs`), no
  supplementary groups, no capabilities.
- SELinux domain `diamaneos_imeiprov`: a QRTR socket, read-only access to the
  traceability block device (`vendor_traceability_block_device`), and the
  SELinux enforce flag it checks before running -- nothing else. `neverallow`
  rules forbid capabilities, any non-QRTR socket, writing any block device,
  setting any property, and Binder.
- An arm64 seccomp allowlist is installed before any modem communication;
  `socket(2)` is limited to QRTR and the local log socket.
- Refuses to run unless SELinux is enforcing.

## Protocol summary

- Service: QRTR service id `0x2ff`, QMI IDL v1 (the Fairphone TCL "TCT_QMI"
  service).
- Message layouts come from the stock binary's QMI IDL message table; the
  service has no standard QMI result TLV, and success is NV status 0.
  - NV read (`0x10`): request TLV `0x01` u32 item id (550). Reply TLV `0x01`
    u32 data length, TLV `0x02` u8 NV status, TLV `0x03` u8[4096] data. Status 5
    (NV_NOTACTIVE) means the item was never written.
  - NV write (`0x30`): request TLV `0x01` u16 item id, TLV `0x02` u32 data
    length (10), TLV `0x03` u8[512] data: the 9-byte NV 550 value, then the
    subscription index (0 or 1). Reply TLV `0x02` u8 NV status.
- Replies use the service's full maximum message size (4110 bytes of TLVs), so
  the receive buffer is 8 KiB and a larger datagram is reported, not truncated.
- NV 550 value: 9 bytes, `[0x08][(d1<<4)|0x0A][(d3<<4)|d2]...[(d15<<4)|d14]`
  (the standard Qualcomm NV_UE_IMEI BCD layout).

The framing is QMI-over-QRTR (no QMUX header) and reuses the codec in
`diamaneos_ims_dcm::protocol`.

## Tests

`imeiprov/tests/decode.rs` covers decoding, Luhn validation, the NV 550 BCD
codec (round-trip and exact bytes), pair validation, and the NV message
build/parse paths, using synthetic IMEIs only (fake all-zero TACs with correct
Luhn digits). Run with the repository host tests (`./tests/run-host-tests.sh`).

## Verifying on a phone (dry run)

With `--check`:

1. Boot the device; the service runs once after persistent properties are ready.
2. Read the result: `adb logcat -d -s imeiprovd` (or run it on demand as root on
   a userdebug build: `adb shell su 0 /vendor/bin/imeiprovd --check`).
3. Expected before provisioning:
   - `trace: slot1_valid=true slot2_valid=true distinct=true`
   - `modem: read_ok=true provisioned=false`

   This proves the traceability decode/validate and the modem NV read both work,
   and that NV 550 was never written (the modem then reports Qualcomm's
   placeholder IMEI). After a `--write` run and a reboot, `--check` shows
   `modem: read_ok=true provisioned=true luhn_valid=true matches_slot1=true`,
   and `*#06#` shows the real IMEIs on both slots. A failed read logs
   `read_ok=false reason=...` with the error kind or NV status only.
