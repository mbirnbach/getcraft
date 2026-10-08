//! Builds the GetCraft app icons from the artwork in `assets/getcraft-source.png`:
//!
//!     cargo run --release -p getcraft --example make_icon [reference.png]
//!
//! The artwork is a rounded turquoise tile on a white background without transparency. This tool
//! finds the tile, cuts it out with an anti-aliased rounded-rectangle mask and scales it to the
//! same margins as the Crafting Apps' icons, so GetCraft sits at the same size next to them in
//! the Dock. Pass a Crafting App's 1024px icon to measure those margins again.
//!
//! Writes `assets/getcraft-1024.png` and `assets/getcraft-macos-256.png` (macOS margins), `assets/getcraft-256.png` and
//! `assets/getcraft-64.png` (full-bleed), `assets/getcraft.ico` and (on macOS, via `iconutil`)
//! `assets/getcraft.icns`.

use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use std::path::Path;
use std::process::Command;

/// Fraction of the canvas the Crafting Apps' macOS icon tiles cover (measured from
/// photocraft-1024.png). Their Windows/Linux icons are full-bleed, and so are ours.
const MACOS_TILE_FRACTION: f32 = 0.8047;

fn is_tile_color(p: &Rgba<u8>) -> bool {
    // The turquoise background: low red, high green and blue.
    p[0] < 110 && p[1] > 160 && p[2] > 190
}

fn is_background(p: &Rgba<u8>) -> bool {
    p[0] > 235 && p[1] > 235 && p[2] > 235
}

/// Bounding box (x0, y0, x1, y1) of the pixels matching `pred`.
fn bbox(img: &RgbaImage, pred: impl Fn(&Rgba<u8>) -> bool) -> (u32, u32, u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if pred(p) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0, y0, x1, y1)
}

/// Estimates the corner radius from how far the tile edge is from a bbox corner, measured
/// along the diagonal: for a circular corner of radius r that distance is r·(√2 − 1).
fn corner_radius(img: &RgbaImage, cx: u32, cy: u32, dx: i32) -> f32 {
    for t in 0..400 {
        let x = cx as i32 + dx * t;
        let y = cy as i32 + t;
        if !is_background(img.get_pixel(x as u32, y as u32)) {
            return (t as f32 * std::f32::consts::SQRT_2) / (std::f32::consts::SQRT_2 - 1.0);
        }
    }
    0.0
}

fn rounded_rect_sdf(x: f32, y: f32, half_w: f32, half_h: f32, r: f32) -> f32 {
    let qx = x.abs() - half_w + r;
    let qy = y.abs() - half_h + r;
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

fn main() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");

    if let Some(reference) = std::env::args().nth(1) {
        let img = image::open(&reference).expect("reference icon").into_rgba8();
        let (x0, _, x1, _) = bbox(&img, |p| p[3] > 128);
        println!("tile covers {:.4} of the canvas", (x1 - x0 + 1) as f32 / img.width() as f32);
        return;
    }

    let src = image::open(assets.join("getcraft-source.png")).expect("assets/getcraft-source.png").into_rgba8();
    let (x0, y0, x1, y1) = bbox(&src, is_tile_color);
    let (w, h) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
    // The top corners show plain turquoise, so they give a clean radius reading.
    let r = (corner_radius(&src, x0, y0, 1) + corner_radius(&src, x1, y0, -1)) / 2.0;
    println!("tile {w}×{h} at ({x0}, {y0}), corner radius ≈ {r:.0}px");

    // Cut out the tile, pulling the edge in slightly to drop the white anti-aliasing fringe.
    let inset = 2.0;
    let (cx, cy) = (x0 as f32 + w / 2.0, y0 as f32 + h / 2.0);
    let size = w.max(h).ceil() as u32;
    let mut tile = RgbaImage::new(size, size);
    for (x, y, out) in tile.enumerate_pixels_mut() {
        let sx = cx - size as f32 / 2.0 + x as f32 + 0.5;
        let sy = cy - size as f32 / 2.0 + y as f32 + 0.5;
        let d = rounded_rect_sdf(sx - cx, sy - cy, w / 2.0 - inset, h / 2.0 - inset, (r - inset).max(0.0));
        let alpha = (0.5 - d).clamp(0.0, 1.0);
        if alpha > 0.0 {
            let p = src.get_pixel((sx as u32).min(src.width() - 1), (sy as u32).min(src.height() - 1));
            *out = Rgba([p[0], p[1], p[2], (alpha * 255.0).round() as u8]);
        }
    }

    // macOS: the tile on a 1024 canvas with the Crafting Apps' margins.
    let tile_px = (1024.0 * MACOS_TILE_FRACTION).round() as u32;
    let scaled = image::imageops::resize(&tile, tile_px, tile_px, FilterType::Lanczos3);
    let mut mac_icon = RgbaImage::new(1024, 1024);
    let offset = ((1024 - tile_px) / 2) as i64;
    image::imageops::overlay(&mut mac_icon, &scaled, offset, offset);
    mac_icon.save(assets.join("getcraft-1024.png")).unwrap();
    // The window icon on macOS becomes the Dock icon, so it needs the macOS margins too.
    image::imageops::resize(&mac_icon, 256, 256, FilterType::Lanczos3)
        .save(assets.join("getcraft-macos-256.png"))
        .unwrap();

    // Windows, Linux and in-app use: full-bleed.
    let resized = |s: u32| image::imageops::resize(&tile, s, s, FilterType::Lanczos3);
    resized(256).save(assets.join("getcraft-256.png")).unwrap();
    resized(64).save(assets.join("getcraft-64.png")).unwrap();

    // Windows: one .ico with the sizes Explorer and the taskbar ask for.
    let frames: Vec<_> = [16, 24, 32, 48, 64, 128, 256]
        .into_iter()
        .map(|s| {
            let img = resized(s);
            image::codecs::ico::IcoFrame::as_png(img.as_raw(), s, s, image::ExtendedColorType::Rgba8).unwrap()
        })
        .collect();
    let ico = std::fs::File::create(assets.join("getcraft.ico")).unwrap();
    image::codecs::ico::IcoEncoder::new(ico).encode_images(&frames).unwrap();

    // macOS: build an .iconset and let iconutil pack it.
    if cfg!(target_os = "macos") {
        let set = std::env::temp_dir().join("GetCraft.iconset");
        let _ = std::fs::remove_dir_all(&set);
        std::fs::create_dir_all(&set).unwrap();
        for base in [16, 32, 128, 256, 512] {
            for scale in [1, 2] {
                let name =
                    if scale == 1 { format!("icon_{base}x{base}.png") } else { format!("icon_{base}x{base}@2x.png") };
                let px = base * scale;
                image::imageops::resize(&mac_icon, px, px, FilterType::Lanczos3).save(set.join(name)).unwrap();
            }
        }
        let status = Command::new("iconutil")
            .args(["-c", "icns", "-o"])
            .arg(assets.join("getcraft.icns"))
            .arg(&set)
            .status()
            .unwrap();
        assert!(status.success(), "iconutil failed");
    }
    println!("icons written to {}", assets.display());
}
