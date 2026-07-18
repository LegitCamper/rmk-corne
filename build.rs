//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.
//!
//! The build script also sets the linker flags to tell it which link script to use.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};

use const_gen::*;
use xz2::read::XzEncoder;

fn main() {
    // Generate the vial config consts at the root of the project (only the
    // central binary uses these, but the build script runs for every binary).
    println!("cargo:rerun-if-changed=vial.json");
    generate_vial_config();

    // Central (Pico W / RP2040) and peripherals (XIAO BLE / nRF52840) are two
    // different target architectures built from this same crate, each with its
    // own memory layout. Pick the right one from the `TARGET` triple cargo
    // passes to the build script.
    let target = env::var("TARGET").unwrap();
    let is_rp2040 = target.starts_with("thumbv6m");
    let memory_x: &[u8] = if is_rp2040 {
        include_bytes!("memory_rp2040.x")
    } else {
        include_bytes!("memory_nrf52840.x")
    };

    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(memory_x)
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying these files
    // here, we ensure the build script is only re-run when they change.
    println!("cargo:rerun-if-changed=memory_nrf52840.x");
    println!("cargo:rerun-if-changed=memory_rp2040.x");
    println!("cargo:rerun-if-env-changed=TARGET");

    // Specify linker arguments.

    // `--nmagic` is required if memory section addresses are not aligned to 0x10000,
    // for example the FLASH and RAM sections in your `memory.x`.
    // See https://github.com/rust-embedded/cortex-m-quickstart/pull/95
    println!("cargo:rustc-link-arg=--nmagic");

    // Set the linker script to the one provided by cortex-m-rt.
    println!("cargo:rustc-link-arg=-Tlink.x");

    // Set the extra linker script from defmt
    println!("cargo:rustc-link-arg=-Tdefmt.x");

    // RP2040's BOOT2 stage needs the extra linker script from embassy-rp.
    if is_rp2040 {
        println!("cargo:rustc-link-arg-bins=-Tlink-rp.x");
        download_cyw43_firmware();
    }
}

fn generate_vial_config() {
    // Generated vial config file
    let out_file = Path::new(&env::var_os("OUT_DIR").unwrap()).join("config_generated.rs");

    let p = Path::new("vial.json");
    let mut content = String::new();
    match File::open(p) {
        Ok(mut file) => {
            file.read_to_string(&mut content)
                .expect("Cannot read vial.json");
        }
        Err(e) => println!("Cannot find vial.json {:?}: {}", p, e),
    };

    let vial_cfg = json::stringify(json::parse(&content).unwrap());
    let mut keyboard_def_compressed: Vec<u8> = Vec::new();
    XzEncoder::new(vial_cfg.as_bytes(), 6)
        .read_to_end(&mut keyboard_def_compressed)
        .unwrap();

    let keyboard_id: Vec<u8> = vec![0x4c, 0x43, 0x52, 0x4e, 0x45, 0x30, 0x30, 0x31];
    let const_declarations = [
        const_declaration!(pub VIAL_KEYBOARD_DEF = keyboard_def_compressed),
        const_declaration!(pub VIAL_KEYBOARD_ID = keyboard_id),
    ]
    .map(|s| "#[allow(clippy::redundant_static_lifetimes)]\n".to_owned() + s.as_str())
    .join("\n");
    fs::write(out_file, const_declarations).unwrap();
}

/// Downloads the CYW43439 wifi/BT firmware blobs the Pico W's `cyw43` driver
/// needs at runtime. Only relevant for the RP2040 central.
fn download_cyw43_firmware() {
    let download_folder = "cyw43-firmware";
    let url_base = "https://github.com/embassy-rs/embassy/raw/refs/heads/main/cyw43-firmware";
    let file_names = [
        "43439A0.bin",
        "43439A0_btfw.bin",
        "43439A0_clm.bin",
        "nvram_rp2040.bin",
        "LICENSE-permissive-binary-license-1.0.txt",
        "README.md",
    ];

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", download_folder);
    fs::create_dir_all(download_folder).expect("Failed to create download directory");

    for file in file_names {
        let url = format!("{}/{}", url_base, file);
        if Path::new(download_folder).join(file).exists() {
            continue;
        }
        match reqwest::blocking::get(&url) {
            Ok(response) => {
                let content = response.bytes().expect("Failed to read file content");
                let file_path = PathBuf::from(download_folder).join(file);
                fs::write(file_path, &content).expect("Failed to write file");
            }
            Err(err) => panic!(
                "Failed to download the cyw43 firmware from {}: {}, required for the central",
                url, err
            ),
        }
    }
}
