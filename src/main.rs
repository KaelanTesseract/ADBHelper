#![windows_subsystem = "windows"]

mod activity;
mod adb;
mod afk_journey;
mod app;
mod charts;
pub mod design_system;
mod localization;
mod routine;
mod settings;
mod updater;

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    localization::Language::load();
    let mut args = env::args().skip(1);
    let command = args.next();
    if command.as_deref() == Some("--apply-update") {
        return match args
            .next()
            .ok_or_else(|| "Update-Auftrag fehlt".to_owned())
            .and_then(|p| updater::apply_update(std::path::Path::new(&p)))
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    if command.as_deref() == Some("check-updates") {
        return match updater::check(&args.next().unwrap_or_else(|| updater::VERSION.into())) {
            Ok(Some(release)) => {
                println!(
                    "Update {}\n{}\n{}",
                    release.version, release.url, release.notes
                );
                ExitCode::SUCCESS
            }
            Ok(None) => {
                println!(
                    "{}",
                    localization::text(
                        "No newer release is available.",
                        "Kein neueres Release verfügbar."
                    )
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    if command.is_none() || command.as_deref() == Some("--update-result") {
        return match eframe::run_native(
            "ADBHelper · AFK Journey",
            eframe::NativeOptions {
                viewport: eframe::egui::ViewportBuilder::default()
                    .with_inner_size([1120.0, 820.0])
                    .with_min_inner_size([900.0, 650.0])
                    .with_icon(app_icon())
                    .with_decorations(false),
                ..Default::default()
            },
            Box::new(|context| Ok(Box::new(app::PlayerApp::new(context)))),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Could not open desktop app: {error}");
                ExitCode::FAILURE
            }
        };
    }
    let mut args = std::iter::once(command.unwrap()).chain(args);
    match args.next().as_deref() {
        Some("devices") => match adb::devices() {
            Ok(devices) if devices.is_empty() => {
                println!("No Android devices found. Check ADB and USB debugging.");
                ExitCode::SUCCESS
            }
            Ok(devices) => {
                for device in devices {
                    println!("{}\t{}", device.serial, device.details);
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
        Some("afk-package") => {
            for package in afk_journey::PACKAGE_NAMES {
                println!("{package}");
            }
            ExitCode::SUCCESS
        }
        Some("afk-check") => {
            let Some(serial) = args.next() else {
                eprintln!("Usage: adbhelper afk-check <device-serial>");
                return ExitCode::from(2);
            };
            match adb::installed_packages(&serial) {
                Ok(packages) => {
                    let found = packages
                        .iter()
                        .find(|package| afk_journey::is_supported_package(package));
                    match found {
                        Some(package) => println!(
                            "{} found on {serial}: {package} (base resolution {}x{}, battle timeout {}s)",
                            afk_journey::DISPLAY_NAME,
                            afk_journey::BASE_RESOLUTION.0,
                            afk_journey::BASE_RESOLUTION.1,
                            afk_journey::BATTLE_TIMEOUT_SECONDS,
                        ),
                        None => println!("{} is not installed on {serial}.", afk_journey::DISPLAY_NAME),
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{error}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("help") | Some("--help") | Some("-h") => {
            println!("Run without arguments to open the desktop app.");
            println!("Usage: adbhelper <devices|afk-package|afk-check <device-serial>>");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Usage: adbhelper <devices|afk-package|afk-check <device-serial>>");
            ExitCode::from(2)
        }
    }
}

fn app_icon() -> eframe::egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/adbhelper-logo-v2.png"))
        .expect("embedded ADBHelper logo should be a valid PNG")
        .to_rgba8();
    let (width, height) = image.dimensions();
    eframe::egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}
