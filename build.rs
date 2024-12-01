// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use base64::prelude::*;
use icns::{IconFamily, IconType, Image};
use ico::{IconDir, IconImage, ResourceType};
use image::imageops;

fn main() {
    // Define the source icon path
    let source_image_path = "assets/images/icon.png";

    // Tell Cargo to rerun this script if the source icon changes
    println!("cargo:rerun-if-changed={}", source_image_path);

    // Load the full-resolution icon source image
    let source_image = image::open(source_image_path).expect("Failed to open source icon");

    let web_dir = Path::new("build/web");
    create_dir_if_not_exists(web_dir);

    // For modern browsers, a 96x96 PNG image.
    resize_and_save(&source_image, &web_dir.join("favicon-96x96.png"), 96);

    // Some browsers will prefer to load a SVG image.
    create_svg_file(&source_image, &web_dir.join("favicon.svg"));

    // And older browsers expect an actual ICO file.
    create_ico_file(&source_image, &web_dir.join("favicon.ico"), &[16, 32, 48]);

    // Apple devices expect a 180x180 PNG image.
    resize_and_save(&source_image, &web_dir.join("apple-touch-icon.png"), 180);

    // Android devices & Chrome expect a 192x192 or 512x512 PNG image.
    resize_and_save(
        &source_image,
        &web_dir.join("web-app-manifest-192x192.png"),
        192,
    );
    resize_and_save(
        &source_image,
        &web_dir.join("web-app-manifest-512x512.png"),
        512,
    );

    // Generate a web app manifest file.
    create_web_app_manifest_file(&web_dir.join("site.webmanifest"));

    // Use the build directory for files that are include!()'d into the Rust code.
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR must be set");

    // Generate icns file for macOS builds.
    let icns_file = Path::new(&out_dir).join("atomCAD-AppIcon.icns");
    create_icns_file(&source_image, &icns_file);
    // Make the icns file available to Rust code.
    println!(
        "cargo:rustc-env=ATOMCAD_ICNS_PATH={}",
        icns_file.to_string_lossy()
    );

    // Write the full-resolution icon to the build directory.
    let icon_file = Path::new(&out_dir).join("atomCAD-icon.png");
    source_image.save(&icon_file).expect("Failed to save icon");

    // Make the icon available to Rust code.
    println!(
        "cargo:rustc-env=ATOMCAD_ICON_PATH={}",
        icon_file.to_string_lossy()
    );
}

fn create_web_app_manifest_file(path: &Path) {
    let mut file = File::create(path).expect("Failed to create web app manifest file");
    file.write_all(b"{\"name\": \"atomCAD\",\"short_name\": \"atomCAD\",\"icons\": [{\"src\": \"/web-app-manifest-192x192.png\",\"sizes\": \"192x192\",\"type\": \"image/png\",\"purpose\": \"maskable\"},{\"src\": \"/web-app-manifest-512x512.png\",\"sizes\": \"512x512\",\"type\": \"image/png\",\"purpose\": \"maskable\"}],\"theme_color\": \"#ffffff\",\"background_color\": \"#ffffff\",\"display\": \"standalone\"}")
        .expect("Failed to write web app manifest file");
}

fn create_dir_if_not_exists(dir: &Path) {
    if !dir.exists() {
        fs::create_dir_all(dir)
            .unwrap_or_else(|_| panic!("Failed to create directory: {}", dir.to_string_lossy()));
    }
}

fn resize_and_save(img: &image::DynamicImage, path: &Path, size: u32) {
    let resized = img.resize(size, size, imageops::FilterType::Lanczos3);
    resized
        .save(path)
        .unwrap_or_else(|_| panic!("Failed to save resized image to {}", path.to_string_lossy()));
}

fn create_svg_file(img: &image::DynamicImage, path: &Path) {
    let width = img.width();
    let height = img.height();
    let mut svg_file = File::create(path).expect("Failed to create favicon SVG file");
    svg_file.write_all(format!("<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><image width=\"{width}\" height=\"{height}\" xlink:href=\"data:image/png;base64,").as_bytes()).expect("Failed to write favicon SVG file prefix");
    svg_file
        .write_all(
            BASE64_STANDARD
                .encode(include_bytes!("assets/images/icon.png"))
                .as_bytes(),
        )
        .expect("Failed to write favicon SVG file image data");
    svg_file
        .write_all(
            br#""></image><style>@media (prefers-color-scheme: light) { :root { filter: none; } }
@media (prefers-color-scheme: dark) { :root { filter: none; } }</style></svg>"#,
        )
        .expect("Failed to write favicon SVG file suffix");
    svg_file.flush().expect("Failed to flush favicon SVG file");
}

// Create ICO with multiple sizes
fn create_ico_file(img: &image::DynamicImage, path: &Path, sizes: &[u32]) {
    // Create a new icon directory
    let mut icon_dir = IconDir::new(ResourceType::Icon);

    // Add each size as an entry in the icon directory
    for &size in sizes {
        // Resize the image to the target size
        let resized = img.resize(size, size, imageops::FilterType::Lanczos3);
        let rgba_image = resized.to_rgba8();

        // Convert RGBA to BGR0 format (which Windows expects)
        let rgba_raw = rgba_image.into_raw();
        let mut bgra_data = Vec::with_capacity(rgba_raw.len());

        // Convert RGBA to BGRA
        for pixel in rgba_raw.chunks_exact(4) {
            bgra_data.push(pixel[2]); // B
            bgra_data.push(pixel[1]); // G
            bgra_data.push(pixel[0]); // R
            bgra_data.push(pixel[3]); // A
        }

        // Create an icon from the BGRA data
        let icon = IconImage::from_rgba_data(size, size, bgra_data);

        // Add the icon to the directory
        icon_dir.add_entry(
            ico::IconDirEntry::encode(&icon)
                .unwrap_or_else(|_| panic!("Failed to encode {size}x{size} icon")),
        );
    }

    // Write the icon directory to the file
    let file = fs::File::create(path).expect("Failed to create ICO file");
    icon_dir.write(file).expect("Failed to write ICO file");
}

// Function to create an ICNS file from a source image
fn create_icns_file(img: &image::DynamicImage, path: &Path) {
    // Create a new icon family
    let mut family = IconFamily::new();

    // 16x16
    let icon16 = img
        .resize(16, 16, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let icns_image16 = Image::from_data(
        icns::PixelFormat::RGBA,
        icon16.width(),
        icon16.height(),
        icon16.into_raw(),
    )
    .expect("Failed to create 16x16 ICNS image");
    family
        .add_icon_with_type(&icns_image16, IconType::RGBA32_16x16)
        .expect("Failed to add 16x16 icon");

    // 32x32
    let icon32 = img
        .resize(32, 32, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let icns_image32 = Image::from_data(
        icns::PixelFormat::RGBA,
        icon32.width(),
        icon32.height(),
        icon32.into_raw(),
    )
    .expect("Failed to create 32x32 ICNS image");
    family
        .add_icon_with_type(&icns_image32, IconType::RGBA32_16x16_2x)
        .expect("Failed to add 32x32 icon");
    family
        .add_icon_with_type(&icns_image32, IconType::RGBA32_32x32)
        .expect("Failed to add 32x32 icon");

    // 64x64
    let icon64 = img
        .resize(64, 64, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let icns_image64 = Image::from_data(
        icns::PixelFormat::RGBA,
        icon64.width(),
        icon64.height(),
        icon64.into_raw(),
    )
    .expect("Failed to create 64x64 ICNS image");
    family
        .add_icon_with_type(&icns_image64, IconType::RGBA32_32x32_2x)
        .expect("Failed to add 64x64 icon");
    family
        .add_icon_with_type(&icns_image64, IconType::RGBA32_64x64)
        .expect("Failed to add 64x64 icon");

    // 128x128
    let icon128 = img
        .resize(128, 128, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let icns_image128 = Image::from_data(
        icns::PixelFormat::RGBA,
        icon128.width(),
        icon128.height(),
        icon128.into_raw(),
    )
    .expect("Failed to create 128x128 ICNS image");
    family
        .add_icon_with_type(&icns_image128, IconType::RGBA32_128x128)
        .expect("Failed to add 128x128 icon");

    // 256x256
    let icon256 = img
        .resize_exact(256, 256, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let icns_image256 = Image::from_data(
        icns::PixelFormat::RGBA,
        icon256.width(),
        icon256.height(),
        icon256.into_raw(),
    )
    .expect("Failed to create 256x256 ICNS image");
    family
        .add_icon_with_type(&icns_image256, IconType::RGBA32_128x128_2x)
        .expect("Failed to add 256x256 icon");
    family
        .add_icon_with_type(&icns_image256, IconType::RGBA32_256x256)
        .expect("Failed to add 256x256 icon");

    // 512x512
    let icon512 = img
        .resize(512, 512, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let icns_image512 = Image::from_data(
        icns::PixelFormat::RGBA,
        icon512.width(),
        icon512.height(),
        icon512.into_raw(),
    )
    .expect("Failed to create 512x512 ICNS image");
    family
        .add_icon_with_type(&icns_image512, IconType::RGBA32_256x256_2x)
        .expect("Failed to add 512x512 icon");
    family
        .add_icon_with_type(&icns_image512, IconType::RGBA32_512x512)
        .expect("Failed to add 512x512 icon");

    // 1024x1024 (added in macOS Big Sur)
    let icon1024 = img
        .resize(1024, 1024, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let icns_image1024 = Image::from_data(
        icns::PixelFormat::RGBA,
        icon1024.width(),
        icon1024.height(),
        icon1024.into_raw(),
    )
    .expect("Failed to create 1024x1024 ICNS image");
    family
        .add_icon_with_type(&icns_image1024, IconType::RGBA32_512x512_2x)
        .expect("Failed to add 1024x1024 icon");

    // Write the icon family to a file
    let file = fs::File::create(path).expect("Failed to create ICNS file");
    let mut writer = BufWriter::new(file);
    family
        .write(&mut writer)
        .expect("Failed to write ICNS file");
}

// End of File
