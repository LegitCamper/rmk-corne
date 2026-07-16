# RMK-Corne 6-Column Build Notes

This configuration is for personal reference, showing the build options for the **Corne 6-column keyboard** with the following specifics:

* **Peripheral halves: Nice!Nano v2**
* **Dongle: Seeed XIAO BLE nRF52840**
* **No rotary encoders**
* **Vial disabled**
* **USB dongle setup**

## Build Options

### RMK_LOG

* Enables central dongle debug logging over usb.
* Usage:

```bash
RMK_LOG=y cargo make uf2 --release
```

### RMK_RESET

* Resets the keyboard on first flash or when pairing new peripherals.
* Usage:

```bash
RMK_RESET=y cargo make uf2 --release
```

### Both Together

```bash
RMK_LOG=y RMK_RESET=y cargo make uf2 --release
```

### Fully wired (no BLE dongle)

By default this repo builds the 3-piece BLE setup described above: two
BLE peripheral halves plus a BLE dongle acting as the USB host. The `wired`
Cargo feature swaps that out for a fully wired 2-piece split:

* **Left half is central** — plugs directly into USB, no dongle.
* **Right half is a wired peripheral** — no BLE, no battery/flash state.
* Halves are linked by a single shared data wire (nice!nano pin **D2**,
  nRF `P0.17`) plus VCC/GND, matching a standard 3-conductor TRS wiring.

Build and flash both halves with:

```bash
cargo make uf2-wired --release
```

This produces `rmk-wired-left.uf2` (flash to the half that plugs into USB)
and `rmk-wired-right.uf2` (flash to the other half). `RMK_RESET=y` works
the same way as above.

**Hardware note:** because D2/`P0.17` is shared for both TX and RX (a
half-duplex single-wire link), each half also hears its own transmissions
echoed back; the firmware discards this automatically. Since both ends can
drive the line, a small series resistor (a few hundred ohms) on the data
line at each end is strongly recommended to limit current if both sides
ever transmit at the same instant.
