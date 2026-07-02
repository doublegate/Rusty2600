//! Host-tooling entry point that generates the Kotlin/Swift bindings for
//! `rusty2600-mobile` from its own compiled metadata. Not shipped to any
//! mobile host; see the `bindgen` feature gate in `Cargo.toml`.

fn main() {
    uniffi::uniffi_bindgen_main();
}
