/// AFK Journey Android package IDs supported by the existing game lifecycle.
pub const PACKAGE_NAMES: &[&str] = &[
    "com.farlightgames.igame.gp",
    "com.farlightgames.igame.gp.vn",
];

pub const DISPLAY_NAME: &str = "AFK Journey";
pub const BASE_RESOLUTION: (u32, u32) = (1080, 1920);
pub const BATTLE_TIMEOUT_SECONDS: u64 = 240;

pub mod afk_stages;

pub fn is_supported_package(package: &str) -> bool {
    PACKAGE_NAMES.contains(&package)
}
