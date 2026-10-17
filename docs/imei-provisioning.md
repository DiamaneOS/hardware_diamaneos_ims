# IMEI provisioning (imeiprovd)

`imeiprovd` is a minimal, source-built, one-shot tool that gives the modem its
IMEIs. DiamaneOS does not ship Fairphone's closed traceability daemon, so the
modem would otherwise report Qualcomm's placeholder IMEI on both slots.
`imeiprovd` does only that daemon's modem-NV part. It writes no Bluetooth or
Wi-Fi MAC files, writes nothing to persist and leaves the region code alone.
`--write` sets one status property (below).

## What it does

1. Opens the `traceability` partition **read-only** and reads just the two
   IMEI fields (15 ASCII digits each).
2. Validates both: exactly 15 digits, a correct Luhn check digit, and the two
   must differ. The modem's unprovisioned placeholder fails the Luhn check, so
   no placeholder constant is in the source.
3. Reads the modem's current NV IMEI (item 550) over the Fairphone TCL QMI
   service on QRTR and compares it with the traceability values.
4. In `--write` mode only: writes the NV item for both subscriptions, in the
   order described [below](#status-and-slot-order), then reads subscription 0
   back to verify (the read has no subscription selector).

It logs only booleans and counters (logcat tag `imeiprovd`), never an IMEI or
other identifier.

## Modes

- `--check`: decode, validate and compare, never write. `--check` and
  `--write` take the modem's QRTR node and use the TCL QMI service only if it
  is on that node, as announced by the local QRTR name service.
- `--write`: the shipped configuration, run once per boot by `imeiprovd.rc`.
  As `--check`, then, only after a successful read, provisions both
  subscriptions and reads back. It writes even when subscription 0 already
  matches: the read cannot show subscription 1, so a boot whose subscription
  1 write failed would otherwise never write it again.
  - Refuses to write unless both traceability IMEIs are 15 digits, pass the Luhn
    check and differ.
  - A failed write or read-back is logged and the tool exits non-zero. It does
    not retry until the next boot.
  - Sets `vendor.diamaneos.imei.status` ([below](#status-and-slot-order)).

Why `--write` runs at every boot: activating a slot (`fastboot --set-active`,
and every OTA) makes the bootloader restore the modem file system from its
golden copy (fsg), which holds no IMEI, so NV 550 is unprovisioned again. A
plain reboot or a bootloader round trip keeps it.
Stock `tctd` writes the IMEIs at every boot for the same reason.

## Status and slot order

On a boot that starts from the modem's golden file system (clean install,
every slot switch) the modem serves subscription 0 on SIM slot 1. On every
later boot it serves subscription 1 there. A modem restart keeps the pairing
of the boot, only a phone restart changes it. Stock writes the traceability
value at 0x24 to subscription 0, which gives the box order on later boots
only. So `--write` picks the pair from what subscription 0 holds:

- Nothing (golden file system): the reversed pair, 0x263 to subscription 0.
  The SIM slots show the box order on this boot. If any step fails it writes
  the normal pair instead.
- The 0x263 value (an earlier boot wrote the reversed pair): the normal pair,
  subscription 1 first, so subscription 0 keeps that marker until both are
  written. The modem then holds the normal pair, as on stock.
- Anything else: the normal pair.

It logs the choice as `order: reversed`, `order: corrected`, `order: normal`
or `order: normal after a failed reversed write`, and sets
`vendor.diamaneos.imei.status` to `running` when it starts, then:

- `reversed`: first write, reversed pair. Box order on this boot.
- `corrected`: the reversed pair from an earlier boot was put back.
- `written`: the modem took the normal pair although it did not hold it
  before. After a clean install or a slot switch the order is swapped until
  the phone restarts.
- `unchanged`: the modem already had the normal pair.
- `failed`: anything else, including a run that ended early.

On the first boot after a clean install (`reversed` with fresh userdata) the
phone process (telephony) offers a restart, as Wi-Fi calling can fail on that
boot until one:

- One notification with Android's translated "Wi-Fi calling" and "Restart your
  device to access the mobile network.", and a Restart action.
- Updates keep userdata, and a factory reset finds the IMEIs in place, so
  neither shows it.
- Nothing restarts the phone without the user.

## Privileges

- Own vendor user/group `vendor_imeiprov` (UID/GID 2994, `config.fs`), with no
  supplementary groups and no capabilities.
- SELinux domain `diamaneos_imeiprov`, allowed only a QRTR socket, read-only
  access to the traceability block device (`vendor_traceability_block_device`),
  the SELinux enforce flag it checks before running, and setting
  `vendor.diamaneos.imei.status` (vendor restricted,
  `vendor_diamaneos_imei_status_prop`).
- Only imeiprovd, init and vendor_init may set the status. Besides imeiprovd,
  only the phone process (`radio`), init, vendor_init and dumpstate may read
  it.
- `neverallow` rules forbid capabilities, any non-QRTR socket, writing any
  block device, setting any other property, and Binder.
- An arm64 seccomp allowlist is installed before any modem communication, and
  `socket(2)` is limited to QRTR and local sockets (logd and the property
  service).
- Refuses to run unless SELinux is enforcing.

## Protocol

- Service: QRTR service id `0x2ff`, QMI IDL v1 (the Fairphone TCL "TCT_QMI"
  service). QMI-over-QRTR framing (no QMUX header), reusing the codec in
  `diamaneos_ims_dcm::protocol`.
- Message layouts come from the stock binary's QMI IDL message table. The
  service has no standard QMI result TLV. Success is NV status 0.
  - NV read (`0x10`): request TLV `0x01` u32 item id (550). Reply TLV `0x01`
    u32 data length, TLV `0x02` u8 NV status, TLV `0x03` u8[4096] data. Status
    5 (NV_NOTACTIVE) means the item was never written.
  - NV write (`0x30`): request TLV `0x01` u16 item id, TLV `0x02` u32 data
    length (10), TLV `0x03` u8[512] data: the 9-byte NV 550 value, then the
    subscription index (0 or 1). Reply TLV `0x02` u8 NV status.
- Replies use the service's full maximum message size (4110 bytes of TLVs), so
  the receive buffer is 8 KiB. A larger datagram is reported, not truncated.
- NV 550 value: 9 bytes, `[0x08][(d1<<4)|0x0A][(d3<<4)|d2]...[(d15<<4)|d14]`
  (the standard Qualcomm NV_UE_IMEI BCD layout).

## Tests

All run with the host tests (`./tests/run-host-tests.sh`), with synthetic
values only: IMEIs with fake all-zero TACs and correct Luhn digits.

- `imeiprov/tests/decode.rs`: decoding, Luhn validation, the NV 550 BCD codec
  (round trip and exact bytes), pair validation and the NV message build/parse
  paths.
- `imeiprov/tests/provision.rs`: the `--write` run with a fake modem, including
  a failed slot 2 write that the next boot writes again, failed reads and the
  reported status.

## Checking on a phone

1. Boot the phone. The service runs once after persistent properties are ready.
2. `adb logcat -d -s imeiprovd` shows the run:
   - `trace: slot1_valid=true slot2_valid=true distinct=true`: the
     traceability decode works.
   - `modem: read_ok=true provisioned=...`: the modem's value before the
     write. It is `provisioned=false` after a slot switch or an OTA.
   - `order: reversed` (or `corrected` on the next boot, `normal` later)
   - `write: sub0_ok=true sub1_ok=true verified=true`
   - `status: reversed` (then `corrected`, then `unchanged`)
3. `*#06#` shows the real IMEIs on both slots.

A failed read logs `read_ok=false reason=...` with the error kind or NV status
only.
