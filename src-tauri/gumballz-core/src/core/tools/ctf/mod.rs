//! CTF utilities and file analysis. All pure Rust and offline: nothing
//! here launches an attack against anyone — it is a hash calculator, format
//! identifier, classic cipher and frequency analysis, what you use to
//! *understand* a file or a text already in your hands.

pub mod analysis;
pub mod ciphers;
pub mod encoding;
pub mod hashes;
pub mod magic;
pub mod xor;
