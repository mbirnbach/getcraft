//! Renders the GetCraft app icon in every format the packages need:
//!
//!     cargo run --release -p getcraft --example render_icon
//!
//! writes `assets/getcraft-1024.png`, `assets/getcraft-256.png`, `assets/getcraft.ico` and (on
//! macOS, via `iconutil`) `assets/getcraft.icns`.

use image::{ImageBuffer, Rgba, RgbaImage, imageops::FilterType};
use std::path::Path;
use std::process::Command;

const SIZE: u32 = 1024;
const SAMPLES: u32 = 4;

type Rgb = [f32; 3];

fn hex(v: u32) -> Rgb {
    [((v >> 16) & 0xff) as f32 / 255.0, ((v >> 8) & 0xff) as f32 / 255.0, (v & 0xff) as f32 / 255.0]
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Signed distance to a rounded rectangle centred at the origin (negative inside).
fn rounded_rect(x: f32, y: f32, half: f32, radius: f32) -> f32 {
    let qx = x.abs() - half + radius;
    let qy = y.abs() - half + radius;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

/// Colour and coverage of the icon at one point (coordinates in 0..SIZE).
fn shade(px: f32, py: f32) -> Option<(Rgb, f32)> {
    let c = SIZE as f32 / 2.0;
    let (x, y) = (px - c, py - c);
    // macOS icon grid: an 824px tile on a 1024px canvas, corner radius ≈ 22.5% of the tile.
    let half = 412.0;
    if rounded_rect(x, y, half, 185.0) > 0.0 {
        return None;
    }
    let t = (y + half) / (2.0 * half);
    let mut color = mix(hex(0x2b2f3d), hex(0x121319), t);

    // The mark: a diamond with a diamond-shaped cut-out, lit from the top left.
    let outer = 270.0;
    let inner = outer * 0.38;
    let d = x.abs() + (y + 8.0).abs();
    // A soft glow behind the mark.
    let glow = (1.0 - (d - outer).max(0.0) / 160.0).clamp(0.0, 1.0).powi(2) * 0.35;
    color = mix(color, hex(0x3d6df2), glow);
    if d <= outer && d >= inner {
        let g = ((x + y) / (2.0 * outer) + 0.5).clamp(0.0, 1.0);
        color = mix(hex(0x8bb8ff), hex(0x3a62e8), g);
        // A thin highlight on the upper-left edges.
        if outer - d < 10.0 && x + y < 0.0 {
            color = mix(color, [1.0, 1.0, 1.0], 0.35);
        }
    }
    Some((color, 1.0))
}

fn render() -> RgbaImage {
    ImageBuffer::from_fn(SIZE, SIZE, |x, y| {
        let mut acc = [0.0f32; 3];
        let mut cover = 0.0;
        for sy in 0..SAMPLES {
            for sx in 0..SAMPLES {
                let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                if let Some((c, a)) = shade(px, py) {
                    for i in 0..3 {
                        acc[i] += c[i] * a;
                    }
                    cover += a;
                }
            }
        }
        if cover == 0.0 {
            return Rgba([0, 0, 0, 0]);
        }
        let n = (SAMPLES * SAMPLES) as f32;
        let rgb = acc.map(|v| ((v / cover) * 255.0).round() as u8);
        Rgba([rgb[0], rgb[1], rgb[2], ((cover / n) * 255.0).round() as u8])
    })
}

fn main() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let icon = render();
    icon.save(assets.join("getcraft-1024.png")).unwrap();
    image::imageops::resize(&icon, 256, 256, FilterType::Lanczos3).save(assets.join("getcraft-256.png")).unwrap();

    // Windows: one .ico with the sizes Explorer and the taskbar ask for.
    let frames: Vec<_> = [16, 24, 32, 48, 64, 128, 256]
        .into_iter()
        .map(|s| {
            let img = image::imageops::resize(&icon, s, s, FilterType::Lanczos3);
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
                let px = base * scale;
                let name =
                    if scale == 1 { format!("icon_{base}x{base}.png") } else { format!("icon_{base}x{base}@2x.png") };
                image::imageops::resize(&icon, px, px, FilterType::Lanczos3).save(set.join(name)).unwrap();
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
