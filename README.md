# RMK-Corne 6-Column Build Notes

This configuration is for personal reference, showing the build options for the **Corne 6-column keyboard** with the following specifics:

* **Peripheral halves: Seeed XIAO BLE (nRF52840)**
* **Central/dongle: Raspberry Pi Pico W (RP2040 + CYW43439)**
* **No rotary encoders**
* **Hardware watchdog enabled**
* **USB dongle setup**

Central and peripherals are two different chip families built from the same
crate (`rmk-corne`), so they target different architectures:

| Binary                              | Board            | Target triple           |
|--------------------------------------|-------------------|--------------------------|
| `central`                            | Pico W (RP2040)   | `thumbv6m-none-eabi`     |
| `peripheral_left` / `peripheral_right` | XIAO BLE (nRF52840) | `thumbv7em-none-eabihf`  |

`cargo make uf2` builds and packages all three; `.cargo/config.toml` and
`Makefile.toml` already pass the right `--target` per binary.

## Battery LED

Each peripheral half blinks its own onboard (red, `P0.26`) LED faster the
lower *its own* battery gets

## Peripheral battery sensing

The peripherals use the XIAO BLE's onboard battery-sense circuit (ADC on
`P0.31`, enabled by holding `P0.14` low) instead of an external voltage
divider. There's no separate charging-status GPIO broken out on this circuit
the way some boards expose one, so only the battery level is reported (no
charging-state detection).

**The ADC calibration constants in `src/peripherals.rs` (`BatteryProcessor::new(2000, 2806)`)
are carried over from a different board's divider and haven't been verified
against the XIAO's actual resistor values** — check reported battery % against
a multimeter reading on real hardware and adjust if it's off.

## Peripheral matrix wiring (XIAO BLE)

Both halves use the same physical pins; column order is mirrored between
left/right so key order comes out correct on each hand. `D10`/`P1.15` is
left free.

| Function | XIAO pin | nRF52840 GPIO |
|----------|----------|----------------|
| Row 0–3  | D0–D3    | P0.02, P0.03, P0.28, P0.29 |
| Col 0–5 (left) / Col 5–0 (right) | D4–D9 | P0.04, P0.05, P1.11, P1.12, P1.13, P1.14 |

## Vial

Vial is enabled (`vial.json` at the project root, compiled in by `build.rs`).
The unlock combo is the two outer corner keys of the top row (matrix
positions `(0,0)` and `(0,11)`); adjust `VialConfig::new(...)` in
`src/central.rs` if you'd rather use different keys.

## rmk / watchdog

`rmk` is pinned to a recent revision that includes hardware watchdog support
(`rmk::watchdog::Rp2040Watchdog` on the central, `rmk::watchdog::Nrf52Watchdog`
on the peripherals), wired into each binary's `run_all!` task list. If either
MCU's firmware hangs, the watchdog resets it automatically.

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

## Flashing

* Peripherals (XIAO BLE) use the Adafruit nRF52 UF2 bootloader — double-tap
  reset (or use the `adafruit_bl` bootloader-jump key) to get to the UF2
  drive, then copy `rmk-peripheral-left.uf2` / `rmk-peripheral-right.uf2` over.
* Central (Pico W) uses the RP2040 UF2 bootloader — hold BOOTSEL while
  plugging in (or double-tap reset once running RMK firmware), then copy
  `rmk-central.uf2` over.

Building the central for the first time requires network access: `build.rs`
downloads the CYW43439 firmware blobs into `./cyw43-firmware/` (matching
upstream `rmk`'s `pi_pico_w_ble_split` example) the first time it's needed.
