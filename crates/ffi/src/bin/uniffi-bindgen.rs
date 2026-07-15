//! The UniFFI binding generator, kept in-crate so its uniffi version always matches the
//! library's. Invoked by `ios/build-rust.sh`:
//!   cargo run -p torg-ffi --bin uniffi-bindgen -- generate --library <dylib> --language swift
fn main() {
    uniffi::uniffi_bindgen_main()
}
