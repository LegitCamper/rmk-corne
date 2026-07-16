#![no_std]
#![no_main]

#[macro_use]
mod macros;

mod keymap;

#[cfg(not(feature = "wired"))]
mod ble;
#[cfg(feature = "wired")]
mod wired;
