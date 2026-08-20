use rusty2600_cart::{detect, Cartridge};
use std::fs;
fn main() {
    for entry in fs::read_dir(std::env::args().nth(1).unwrap()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map(|e| e == "a26").unwrap_or(false) {
            let rom = fs::read(&path).unwrap();
            let result = detect(&rom);
            let name = path.file_name().unwrap().to_string_lossy();
            match result {
                Some(c) => println!("{} ({} bytes): {:?}", name, rom.len(), variant_name(&c)),
                None => println!("{} ({} bytes): None (unsupported)", name, rom.len()),
            }
        }
    }
}
fn variant_name(c: &Cartridge) -> &'static str {
    match c {
        Cartridge::Rom2K(_) => "Rom2K",
        Cartridge::Rom4K(_) => "Rom4K",
        Cartridge::BankF8(_) => "BankF8",
        Cartridge::BankF6(_) => "BankF6",
        Cartridge::BankF4(_) => "BankF4",
        Cartridge::BankCV(_) => "BankCV",
        Cartridge::BankFA(_) => "BankFA",
        Cartridge::BankDpc(_) => "BankDpc",
        Cartridge::BankE7(_) => "BankE7",
    }
}
