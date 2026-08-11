# RMK-Corne 6-Column Build Notes

This configuration is for personal reference, showing the build options for the **Corne 6-column keyboard** with the following specifics:

* **All three boards: Seeed XIAO BLE (nRF52840)** -- two keyboard halves plus
  a third XIAO acting as a matrix-less USB dongle for the central role
* **No rotary encoders**
* **Hardware watchdog enabled**
* **USB dongle setup**

Central and peripherals are all the same chip (nRF52840) built from the same
crate (`rmk-corne`), so all three binaries target the same architecture
(`thumbv7em-none-eabihf`). The central has no matrix of its own -- it just
bridges USB (to the host) and BLE (to the two split halves).

`cargo make uf2` builds and packages all three.

## Status LEDs (XIAO BLE onboard RGB LED)

Each peripheral half drives all three channels of its onboard RGB LED:

* **Red (`P0.26`)** blinks faster the lower *its own* battery gets.
* **Green (`P0.30`)** flashes a few times right at boot as an "I'm alive"
  signal, then hands off to blue.
* **Blue (`P0.06`)** blinks while the half is advertising/trying to
  (re)connect to the central, and turns off once connected.

The central (dongle) drives its own **blue (`P0.06`)** the same way, but from
the other side of the link: it blinks while either split peripheral isn't
connected -- at boot, or if a half dies/goes out of range later -- and turns
off once both are connected. A blinking dongle after boot means a half is
missing.

## Peripheral battery sensing

The peripherals use the XIAO BLE's onboard battery-sense circuit (ADC on
`P0.31`, enabled by holding `P0.14` low) instead of an external voltage
divider. There's no separate charging-status GPIO broken out on this circuit
the way some boards expose one, so only the battery level is reported (no
charging-state detection).

## Peripheral matrix wiring (XIAO BLE)

Pins match this board's actual schematic/ZMK reference config
([JonMuller/gerbers corne-choc-xiao](https://github.com/JonMuller/gerbers/tree/main/corne-choc-xiao)),
not a generic XIAO `D0`-`D10` breakout numbering. Column order is mirrored
between left/right so key order comes out correct on each hand. `col0` is
the NFC2 pin (`P0.10`), usable as GPIO via the `nfc-pins-as-gpio`
`embassy-nrf` feature already enabled.

| Function | nRF52840 GPIO |
|----------|----------------|
| Row 0–3  | P0.02, P0.03, P0.28, P0.29 |
| Col 0–5 (left)  | P0.10, P1.15, P1.14, P1.13, P1.12, P1.11 |
| Col 0–5 (right) | P1.11, P1.12, P1.13, P1.14, P1.15, P0.10 |

## rmk / watchdog

`rmk` is pinned to a recent revision that includes hardware watchdog support
(`rmk::watchdog::Nrf52Watchdog`, wired into each binary's `run_all!` task
list). If any of the three MCUs' firmware hangs, the watchdog resets it
automatically.

## Build Options

### RMK_LOG

* Enables central dongle debug logging over usb.
* Usage:

```bash
RMK_LOG=y cargo make uf2
```

### RMK_RESET

* Resets the keyboard on first flash or when pairing new peripherals.
* Usage:

```bash
RMK_RESET=y cargo make uf2
```

### Both Together

```bash
RMK_LOG=y RMK_RESET=y cargo make uf2
```

## Flashing

All three boards (central, `peripheral_left`, `peripheral_right`) use the
Adafruit nRF52 UF2 bootloader — double-tap reset (or use the `adafruit_bl`
bootloader-jump key) to get to the UF2 drive, then copy the matching
`rmk-*.uf2` file over.
