use image::codecs::ico::{IcoEncoder, IcoFrame};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/adbhelper-logo-v2.png");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let logo = image::open("assets/adbhelper-logo-v2.png").expect("decode application logo");
    let mut frames = Vec::new();
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let scaled = logo
            .resize(size, size, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let mut canvas = image::RgbaImage::new(size, size);
        image::imageops::overlay(
            &mut canvas,
            &scaled,
            i64::from((size - scaled.width()) / 2),
            i64::from((size - scaled.height()) / 2),
        );
        frames.push(
            IcoFrame::as_png(canvas.as_raw(), size, size, image::ExtendedColorType::Rgba8)
                .expect("encode icon size"),
        );
    }
    let icon = out.join("adbhelper.ico");
    IcoEncoder::new(fs::File::create(&icon).expect("create icon"))
        .encode_images(&frames)
        .expect("encode application icon");
    let resource = out.join("adbhelper.rc");
    fs::write(
        &resource,
        format!(
            "1 ICON \"{}\"\n",
            icon.display().to_string().replace('\\', "/")
        ),
    )
    .expect("write icon resource");
    embed_resource::compile_for(&resource, &["adbhelper"], embed_resource::NONE)
        .manifest_required()
        .expect("embed Windows application icon");
}
