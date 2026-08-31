//! Reads the active Omarchy palette and converts it into something a neon
//! vector game can actually use.
//!
//! Omarchy themes are tuned for eight hours of comfortable reading: mid
//! contrast, restrained chroma. Rendered literally, every theme produces a
//! washed-out grey game. So we keep each colour's *hue* -- which is what makes
//! Everforest recognisably Everforest -- and force lightness and chroma to
//! fixed neon targets in OKLCH.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Near-black, carrying a trace of the theme's background hue.
    pub void: [f32; 3],
    /// Terrain, grid, obstacles, own HUD.
    pub primary: [f32; 3],
    /// Enemies and anything that wants shooting.
    pub hostile: [f32; 3],
    /// Radar sweep, stars, secondary HUD.
    pub accent: [f32; 3],
}

impl Default for Palette {
    /// Classic-arcade fallback, used when no Omarchy install is found.
    fn default() -> Self {
        Self {
            void: [0.0, 0.0, 0.0],
            primary: neon([0.21, 1.0, 0.55]),
            hostile: neon([1.0, 0.30, 0.24]),
            accent: neon([0.54, 1.0, 0.76]),
        }
    }
}

/// Omarchy moved this path between releases, so try the current location first
/// and fall back to the older one.
pub fn theme_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let home = Path::new(&home);
    let candidates = [
        home.join(".local/state/omarchy/current/theme"),
        home.join(".config/omarchy/current/theme"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

pub fn load() -> Result<Palette> {
    let dir = theme_dir().ok_or_else(|| anyhow!("no omarchy theme directory found"))?;
    let text = std::fs::read_to_string(dir.join("colors.toml"))?;
    Ok(from_colors_toml(&text))
}

/// Deliberately permissive. Rather than binding to one schema version, flatten
/// the whole document to (dotted key, hex) pairs and match on key names, with a
/// chroma-ranked fallback if nothing matches. A theme that renames its keys
/// degrades to "slightly different colours", not a crash.
pub fn from_colors_toml(text: &str) -> Palette {
    let mut pairs: Vec<(String, [f32; 3])> = Vec::new();
    if let Ok(value) = text.parse::<toml::Value>() {
        flatten(&value, String::new(), &mut pairs);
    }

    let pick = |names: &[&str]| -> Option<[f32; 3]> {
        for n in names {
            if let Some((_, c)) = pairs
                .iter()
                .find(|(k, _)| k.rsplit('.').next().map(|s| s == *n).unwrap_or(false))
            {
                return Some(*c);
            }
        }
        None
    };

    let bg = pick(&["background", "bg", "base", "color0"]).unwrap_or([0.02, 0.02, 0.03]);
    let fg = pick(&["cyan", "blue", "foreground", "fg", "text", "color6", "color4"]);
    let hot = pick(&["red", "orange", "color1", "color9"]);
    let acc = pick(&["magenta", "purple", "violet", "color5", "color13"]);

    // Chroma-ranked leftovers for anything the name match missed.
    let mut ranked = pairs.clone();
    ranked.sort_by(|a, b| chroma(b.1).total_cmp(&chroma(a.1)));
    let nth = |i: usize| ranked.get(i).map(|(_, c)| *c).unwrap_or([0.5, 0.5, 0.5]);

    Palette {
        void: crush(bg),
        primary: neon(fg.unwrap_or_else(|| nth(0))),
        hostile: neon(hot.unwrap_or_else(|| nth(1))),
        accent: neon(acc.unwrap_or_else(|| nth(2))),
    }
}

fn flatten(v: &toml::Value, prefix: String, out: &mut Vec<(String, [f32; 3])>) {
    match v {
        toml::Value::Table(t) => {
            for (k, v) in t {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten(v, key, out);
            }
        }
        toml::Value::String(s) => {
            if let Some(rgb) = parse_hex(s) {
                out.push((prefix, rgb));
            }
        }
        _ => {}
    }
}

fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 && h.len() != 8 {
        return None;
    }
    let n = u32::from_str_radix(&h[..6], 16).ok()?;
    Some([
        ((n >> 16) & 0xff) as f32 / 255.0,
        ((n >> 8) & 0xff) as f32 / 255.0,
        (n & 0xff) as f32 / 255.0,
    ])
}

// ---------------------------------------------------------------- OKLab ----

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_oklab(c: [f32; 3]) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn oklab_to_linear(c: [f32; 3]) -> [f32; 3] {
    let (ll, aa, bb) = (c[0], c[1], c[2]);
    let l = (ll + 0.3963377774 * aa + 0.2158037573 * bb).powi(3);
    let m = (ll - 0.1055613458 * aa - 0.0638541728 * bb).powi(3);
    let s = (ll - 0.0894841775 * aa - 1.2914855480 * bb).powi(3);
    [
        (4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s).clamp(0.0, 1.0),
        (-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s).clamp(0.0, 1.0),
        (-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s).clamp(0.0, 1.0),
    ]
}

fn chroma(srgb: [f32; 3]) -> f32 {
    let lab = linear_to_oklab(srgb.map(srgb_to_linear));
    (lab[1] * lab[1] + lab[2] * lab[2]).sqrt()
}

/// Keep the hue, force lightness and chroma to neon. Colours that arrive
/// near-grey get a hue invented from nothing useful, so they are pushed toward
/// the theme's most saturated direction rather than staying grey.
pub fn neon(srgb: [f32; 3]) -> [f32; 3] {
    let lab = linear_to_oklab(srgb.map(srgb_to_linear));
    let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let h = lab[2].atan2(lab[1]);
    let target_c = (c * 2.4).clamp(0.14, 0.24);
    oklab_to_linear([0.82, target_c * h.cos(), target_c * h.sin()])
}

/// The void keeps a hint of the theme's background hue but almost none of its
/// lightness -- bloom needs somewhere genuinely dark to bloom into.
fn crush(srgb: [f32; 3]) -> [f32; 3] {
    let lab = linear_to_oklab(srgb.map(srgb_to_linear));
    oklab_to_linear([0.10, lab[1] * 0.25, lab[2] * 0.25])
}

// --------------------------------------------------------------- watcher ----

/// Watches the theme directory and yields a fresh palette whenever Omarchy
/// swaps themes. Returns None if there is no Omarchy install to watch.
pub fn watch() -> Option<Receiver<Palette>> {
    use notify::{RecursiveMode, Watcher};

    let dir = theme_dir()?;
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let (raw_tx, raw_rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(raw_tx) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("theme watcher failed to start: {e}");
                return;
            }
        };
        // The current/theme path is itself replaced on switch, so watch the
        // parent too or the watch handle goes stale after one change.
        let _ = watcher.watch(&dir, RecursiveMode::NonRecursive);
        if let Some(parent) = dir.parent() {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
        }

        for event in &raw_rx {
            if event.is_err() {
                continue;
            }
            // Omarchy writes several files per switch; settle before reading.
            std::thread::sleep(std::time::Duration::from_millis(120));
            while raw_rx.try_recv().is_ok() {}
            if let Ok(p) = load() {
                if tx.send(p).is_err() {
                    return;
                }
            }
        }
    });

    Some(rx)
}
