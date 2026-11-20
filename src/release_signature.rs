//! Release signature format, shared by the app (verifying updates) and
//! `examples/release-signing.rs` (signing releases on the maintainer's PC).

/// Suffix of the signature file published next to each release file.
pub const SUFFIX: &str = ".minisig";

/// The signed trusted comment. It binds a signature to one file name and one
/// version, so an older genuine EXE cannot be offered as a newer update.
pub fn trusted_comment(file_name: &str, version: &str) -> String {
    format!("ADBHelper {} {file_name}", version.trim_start_matches('v'))
}
