//! Release signing on the maintainer's PC. The secret key never leaves this PC
//! and must never be uploaded, committed, or stored as a GitHub secret.
//!
//! ```powershell
//! # once: create the key pair (asks for a password twice)
//! cargo run --release --example release-signing -- keygen
//! # per release file (asks for the password)
//! cargo run --release --example release-signing -- sign dist\ADBHelper-Windows-x64.exe 0.4.11
//! ```
//!
//! The secret key is kept in `%USERPROFILE%\.adbhelper-signing\minisign.key`
//! (override with `ADBHELPER_SIGNING_KEY`); the public key goes to `minisign.pub`
//! in the repository, where the app embeds it.

#[path = "../src/release_signature.rs"]
mod release_signature;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn secret_key_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("ADBHELPER_SIGNING_KEY") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or("No user profile folder found. Set ADBHELPER_SIGNING_KEY.")?;
    Ok(PathBuf::from(home)
        .join(".adbhelper-signing")
        .join("minisign.key"))
}

fn public_key_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("minisign.pub")
}

fn keygen() -> Result<(), String> {
    let secret = secret_key_path()?;
    if secret.exists() {
        return Err(format!(
            "{} already exists. Remove it yourself only if you really want a new key; \
             every installed ADBHelper then needs an update signed with the old key first.",
            secret.display()
        ));
    }
    std::fs::create_dir_all(secret.parent().ok_or("Invalid key path")?)
        .map_err(|e| e.to_string())?;
    let mut public = Vec::new();
    let mut secret_bytes = Vec::new();
    println!("Choose a password for the ADBHelper release key. It is asked for at every signing.");
    minisign::KeyPair::generate_and_write_encrypted_keypair(
        &mut public,
        &mut secret_bytes,
        Some("ADBHelper release signing key"),
        None,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(&secret, secret_bytes).map_err(|e| e.to_string())?;
    std::fs::write(public_key_path(), public).map_err(|e| e.to_string())?;
    println!(
        "Secret key: {} (keep a private backup, never upload it)",
        secret.display()
    );
    println!(
        "Public key: {} (commit this file)",
        public_key_path().display()
    );
    Ok(())
}

fn sign(file: &Path, version: &str) -> Result<(), String> {
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid file name")?;
    let secret = minisign::SecretKey::from_file(secret_key_path()?, None)
        .map_err(|e| format!("Could not open the secret key: {e}"))?;
    let public = minisign::PublicKey::from_file(public_key_path()).map_err(|e| e.to_string())?;
    let comment = release_signature::trusted_comment(name, version);
    let data = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let signature = minisign::sign(Some(&public), &secret, data, Some(&comment), None)
        .map_err(|e| e.to_string())?;
    let output = PathBuf::from(format!("{}{}", file.display(), release_signature::SUFFIX));
    std::fs::write(&output, signature.to_string()).map_err(|e| e.to_string())?;
    // Check the result exactly like the app will.
    minisign_verify::PublicKey::from_file(public_key_path())
        .and_then(|key| {
            let signature = minisign_verify::Signature::from_file(&output)?;
            key.verify(&std::fs::read(file)?, &signature, false)
        })
        .map_err(|e| format!("The new signature does not verify: {e}"))?;
    println!("Signed {name} for version {version}: {}", output.display());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["keygen"] => keygen(),
        ["sign", file, version] => sign(Path::new(file), version),
        _ => Err("Usage: release-signing keygen | release-signing sign <file> <version>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
