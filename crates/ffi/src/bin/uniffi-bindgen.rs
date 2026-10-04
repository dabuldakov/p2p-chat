//! Генератор Kotlin-биндингов из скомпилированной библиотеки.
//!
//! Пример:
//!   cargo build -p p2pchat-ffi
//!   cargo run -p p2pchat-ffi --bin uniffi-bindgen -- \
//!     generate --library target/debug/libp2pchat_ffi.so \
//!     --language kotlin --out-dir android/app/src/main/java

fn main() {
    uniffi::uniffi_bindgen_main()
}