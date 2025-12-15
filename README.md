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
