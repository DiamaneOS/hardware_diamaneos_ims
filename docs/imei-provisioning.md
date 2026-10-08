# IMEI provisioning (imeiprovd)

`imeiprovd` is a minimal, source-built, one-shot tool that gives the modem its
IMEIs. DiamaneOS does not ship Fairphone's closed traceability daemon, so the
modem would otherwise report Qualcomm's placeholder IMEI on both slots.
`imeiprovd` does only three of that daemon's jobs: the modem-NV part and, in
separate modes, the factory Bluetooth address and Wi-Fi MAC (see below). It
writes nothing to persist, leaves the region code alone, and its IMEI modes set
no properties.

## What it does

1. Opens the `traceability` partition **read-only** and reads just the two
   IMEI fields (15 ASCII digits each).
2. Validates both: exactly 15 digits, a correct Luhn check digit, and the two
   must differ. The modem's unprovisioned placeholder fails the Luhn check, so
   no placeholder constant is in the source.
3. Reads the modem's current NV IMEI (item 550) over the Fairphone TCL QMI
   service on QRTR and compares it with the traceability values.
4. In `--write` mode only: writes the NV item for both subscriptions, then
   reads slot 1 back to verify (the read has no subscription selector).

It logs only booleans and counters (logcat tag `imeiprovd`), never an IMEI or
other identifier.

## Modes

- `--check`: decode, validate and compare; never writes. For dry runs
  (`adb shell 'su 0 /vendor/bin/imeiprovd --check $(getprop ro.vendor.diamaneos.ims.modem_node)'`
  on a userdebug build). `--check` and `--write` take the modem's QRTR node and
  use the TCL QMI service only if it is on that node, as announced by the local
  QRTR name service.
- `--write`: the shipped configuration; `imeiprovd.rc` runs it once per boot.
  As `--check`, then, only after a successful read, provisions both
  subscriptions and reads back. It writes even when slot 1 already matches:
  the read cannot show slot 2, so a boot whose slot 2 write failed would
  otherwise never write it again.
  - Refuses to write unless both traceability IMEIs are 15 digits, pass the Luhn
    check and differ.
  - A failed write or read-back is logged and the tool exits non-zero; no retry
    until the next boot.
- `--bt-address`: the factory Bluetooth address; `imeiprovd.rc` runs it once
  per boot as its own service (below). Touches no IMEI and no modem.
- `--wlan-mac`: the factory Wi-Fi MAC; `imeiprovd.rc` runs it once per boot as
  its own service (below). Touches no IMEI and no modem.

Why `--write` runs at every boot: activating a slot (`fastboot --set-active`,
and every OTA) makes the bootloader restore the modem file system from its
golden copy (fsg), which holds no IMEI, so NV 550 is unprovisioned again. Seen
on the FP6 on 2026-10-05; a plain reboot or a bootloader round trip keeps it.
Stock `tctd` writes the IMEIs at every boot for the same reason.

## Bluetooth address (`--bt-address`)

Without the closed daemon, Qualcomm's HCI implementation found no address and
generated one: the prefix 22:22 plus four bytes from `rand()`, stored in
`persist.vendor.service.bdroid.bdaddr`, which marks a DiamaneOS phone. The
factory address is in the traceability partition, so `vendor.imeiprovd-bt`
(started at `post-fs`):

1. Reads the 6 bytes at offset `0x33` (right after the slot 1 IMEI), read-only.
   They are stored least significant byte first; stock prints them in reverse
   as `xx:xx:xx:xx:xx:xx` (`tctd` @0xb3cc).
2. Validates them. Stock checks nothing; `imeiprovd` rejects all-zero and
   all-`0xff` values, group (multicast) and locally administered addresses
   (which includes the generated 22:22 prefix), and the Bluetooth inquiry
   LAPs `0x9E8B00`-`0x9E8B3F`, which no device address may use.
3. Sets `ro.vendor.diamaneos.bt.factory_address` (lowercase, colon-separated,
   as stock). The device's Bluetooth init rc copies it to
   `ro.vendor.bt.boot.macaddr`, as stock's `tctd.rc` does; the HCI
   implementation reads that before falling back to its stored or a newly
   generated address.

It logs `bt address: set`, `bt address: missing` (read failed),
`bt address: invalid` or `bt address: property not set`, never the address.
If the address is missing or invalid nothing is set, and Bluetooth keeps the
address it used before.

## Wi-Fi MAC (`--wlan-mac`)

Without the closed daemon, nothing wrote the WLAN driver's MAC file, so the
QCA6750 driver used its firmware's generic Qualcomm address (prefix 00:03:7f)
instead of the factory MAC. Stock: `tctd` sets `ro.vendor.trace.wifimac` and
`setwlanmac.sh`, a late_start script, copies it to
`/mnt/vendor/persist/qca6750/wlan_mac.bin`, which a `/vendor/firmware` link
exposes to the driver, so a new or changed MAC takes effect only from the next
boot. `vendor.imeiprovd-wlan` (init waits for it at `post-fs`):

1. Reads the 6 bytes at offset `0x39` (right after the Bluetooth address),
   read-only, stored least significant byte first, as for the address.
2. Validates them: not all zero, not all `0xff` (broadcast), not a group
   (multicast) and not a locally administered address. Stock checks nothing.
3. Creates `/mnt/vendor/wlan_mac/wlan/qca_cld/qca6750/wlan_mac.bin` (tmpfs,
   new file, mode 0400, no link followed) with stock's 33 bytes:
   `Intf0MacAddress=<12 lowercase hex digits>`, a newline, `END`, a newline.

The driver asks for `wlan/qca_cld/qca6750/wlan_mac.bin` when it probes, after
early-boot starts the Wi-Fi processor. Its configuration has
`read_mac_addr_from_mac_file=1` (as stock). The kernel finds no such file on
its own path, ueventd finds it in `/mnt/vendor/wlan_mac/` (a firmware directory
in the device's `ueventd.rc`), and the driver takes `Intf0` as the station's
hardware address and generates the others (P2P and so on). Android randomises
the address per network on top of it.

It logs `wlan mac: written`, `wlan mac: missing` (read failed), `wlan mac:
invalid` or `wlan mac: file not written: <error kind>`, never the MAC. If the
MAC is missing or invalid no file is written and the driver uses its firmware's
address, as before.

## Privileges

- Own vendor user/group `vendor_imeiprov` (UID/GID 2994, `config.fs`); no
  supplementary groups, no capabilities.
- SELinux domain `diamaneos_imeiprov`, allowed only a QRTR socket, read-only
  access to the traceability block device (`vendor_traceability_block_device`),
  the SELinux enforce flag it checks before running, setting
  `ro.vendor.diamaneos.bt.factory_address` (its own vendor-internal type,
  `vendor_diamaneos_bt_address_prop`), and creating and writing the Wi-Fi MAC
  file (`vendor_diamaneos_wlan_mac_file`, which also labels its directories).
- `neverallow` rules forbid capabilities, any non-QRTR socket, writing any
  block device, setting any other property, adding names to any other
  directory, reading the MAC file back, and Binder.
- Others keep every domain except init and vendor_init from setting the
  address property or writing the MAC file, every domain except init,
  vendor_init (the copy) and dumpstate from reading the property, and (in the
  device policy) every domain except ueventd, init and vendor_init from
  reading the MAC file.
- An arm64 seccomp allowlist is installed before any modem communication;
  `socket(2)` is limited to QRTR and the local log socket. `--bt-address` and
  `--wlan-mac` install none: each reads 6 bytes, makes one property call or
  writes one file, and exits.
- Refuses to run unless SELinux is enforcing.

## Protocol

- Service: QRTR service id `0x2ff`, QMI IDL v1 (the Fairphone TCL "TCT_QMI"
  service). QMI-over-QRTR framing (no QMUX header), reusing the codec in
  `diamaneos_ims_dcm::protocol`.
- Message layouts come from the stock binary's QMI IDL message table. The
  service has no standard QMI result TLV; success is NV status 0.
  - NV read (`0x10`): request TLV `0x01` u32 item id (550). Reply TLV `0x01`
    u32 data length, TLV `0x02` u8 NV status, TLV `0x03` u8[4096] data. Status
    5 (NV_NOTACTIVE) means the item was never written.
  - NV write (`0x30`): request TLV `0x01` u16 item id, TLV `0x02` u32 data
    length (10), TLV `0x03` u8[512] data: the 9-byte NV 550 value, then the
    subscription index (0 or 1). Reply TLV `0x02` u8 NV status.
- Replies use the service's full maximum message size (4110 bytes of TLVs), so
  the receive buffer is 8 KiB; a larger datagram is reported, not truncated.
- NV 550 value: 9 bytes, `[0x08][(d1<<4)|0x0A][(d3<<4)|d2]...[(d15<<4)|d14]`
  (the standard Qualcomm NV_UE_IMEI BCD layout).

## Tests

`imeiprov/tests/decode.rs` covers decoding, Luhn validation, the NV 550 BCD
codec (round trip and exact bytes), pair validation and the NV message
build/parse paths, with synthetic IMEIs only (fake all-zero TACs with correct
Luhn digits). `imeiprov/tests/bdaddr.rs` covers the Bluetooth address byte
order, property format, validation and redaction, and
`imeiprov/tests/wlanmac.rs` the Wi-Fi MAC's byte order, the exact MAC file
(read back with a copy of the driver's parser), validation and redaction,
with addresses from the RFC 7042 documentation range only. All run with the
host tests (`./tests/run-host-tests.sh`).

## Checking on a phone

1. Boot the phone; the service runs once after persistent properties are ready.
2. Read the result: `adb logcat -d -s imeiprovd`. Or run a dry run as root on a
   userdebug build: `adb shell 'su 0 /vendor/bin/imeiprovd --check $(getprop ro.vendor.diamaneos.ims.modem_node)'`.
3. Before provisioning, `--check` shows:
   - `trace: slot1_valid=true slot2_valid=true distinct=true`
   - `modem: read_ok=true provisioned=false`

   So the traceability decode and the modem NV read work, and NV 550 was never
   written (the modem reports Qualcomm's placeholder IMEI).
4. After a `--write` run and a reboot, `--check` shows
   `modem: read_ok=true provisioned=true luhn_valid=true matches_slot1=true`,
   and `*#06#` shows the real IMEIs on both slots.

A failed read logs `read_ok=false reason=...` with the error kind or NV status
only.

For the Bluetooth address: `adb logcat -d -s imeiprovd` shows `bt address: set`.
As root, compare only the first 8 characters (the manufacturer prefix) of
`getprop ro.vendor.bt.boot.macaddr` with those of the factory file stock left
in `/mnt/vendor/persist/trace_info/bt_macaddr`; they match, and neither starts
with `22:22`.

For the Wi-Fi MAC: `adb logcat -d -s imeiprovd` shows `wlan mac: written`, and
the kernel log shows ueventd's `found /mnt/vendor/wlan_mac/wlan/qca_cld/qca6750/wlan_mac.bin`
and the driver's `using MAC address from wlan_mac.bin`. As root, compare only
the first 6 hex digits after `Intf0MacAddress=` of that file with those of
stock's `/mnt/vendor/persist/qca6750/wlan_mac.bin`.
