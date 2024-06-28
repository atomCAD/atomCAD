// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file,
// You can obtain one at <https://mozilla.org/MPL/2.0/>.

use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use base64::prelude::*;
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

// End of File
