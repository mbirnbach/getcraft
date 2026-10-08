//! The GetCraft mark (a diamond with a diamond-shaped cut-out), rasterised in code for the tray
//! icon. The full-colour app icon is rendered by `examples/render_icon.rs`.

/// Renders the mark as RGBA pixels. `fill` is used for the diamond; everything else is
/// transparent. `inset` (0..1) is the size of the cut-out relative to the diamond.
pub fn rgba(size: u32, fill: [u8; 3], margin: f32, inset: f32) -> Vec<u8> {
    const SAMPLES: u32 = 4;
    let s = size as f32;
    let center = s / 2.0;
    let outer = s / 2.0 - margin * s;
    let inner = outer * inset;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut hits = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32 - center;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32 - center;
                    // A diamond is |x| + |y| <= r.
                    let d = px.abs() + py.abs();
                    if d <= outer && d >= inner {
                        hits += 1;
                    }
                }
            }
            let alpha = hits * 255 / (SAMPLES * SAMPLES);
            let i = ((y * size + x) * 4) as usize;
            out[i..i + 3].copy_from_slice(&fill);
            out[i + 3] = alpha as u8;
        }
    }
    out
}

/// Monochrome menu-bar/tray version. On macOS it's used as a template image, so the system
/// tints it to match light or dark menu bars.
pub fn tray(size: u32) -> Vec<u8> {
    let color = if cfg!(target_os = "macos") { [0, 0, 0] } else { [0x4f, 0x8c, 0xff] };
    rgba(size, color, 0.06, 0.38)
}
