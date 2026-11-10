//! Omarchy theme set: `themes/<name>.toml` (verbatim Omarchy `colors.toml`) compiled in and
//! mapped onto the app's palette roles.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Where a block sits relative to the current time; sets how bright it is drawn.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Already over: dimmest.
    Past,
    /// Not started yet: medium.
    Future,
    /// Under way right now: the full accent.
    Now,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Rgb,
    pub fg: Rgb,
    /// Gridlines and secondary text.
    pub dim: Rgb,
    pub accent: Rgb,
    pub selection: Rgb,
    pub error: Rgb,
    pub is_light: bool,
}

impl Theme {
    /// Opaque block body color, the same for every kind (the corner marker tells kinds
    /// apart): the theme accent mixed over the background, by where the block sits in time.
    pub fn block_fill(&self, tone: Tone) -> Rgb {
        let w = match tone {
            Tone::Past => 0.3,
            Tone::Future => 0.6,
            Tone::Now => 1.0,
        };
        let (a, b) = (self.accent, self.bg);
        let m = |x: u8, y: u8| (w * x as f32 + (1.0 - w) * y as f32).round() as u8;
        Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
    }

    /// Whichever of `fg`/`bg` reads better on `fill` (higher WCAG contrast).
    pub fn text_on(&self, fill: Rgb) -> Rgb {
        if contrast(self.fg, fill) >= contrast(self.bg, fill) {
            self.fg
        } else {
            self.bg
        }
    }
}

macro_rules! sources {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../themes/", $name, ".toml")))),*]
    };
}

const SOURCES: &[(&str, &str)] = sources!(
    "catppuccin-latte",
    "catppuccin",
    "ethereal",
    "everforest",
    "flexoki-light",
    "gruvbox",
    "hackerman",
    "kanagawa",
    "lumon",
    "matte-black",
    "miasma",
    "nord",
    "osaka-jade",
    "retro-82",
    "ristretto",
    "rose-pine",
    "tokyo-night",
    "vantablack",
    "white",
);

const DEFAULT_THEME: &str = "tokyo-night";

/// All bundled themes, in a fixed order.
pub fn themes() -> &'static [Theme] {
    static THEMES: OnceLock<Vec<Theme>> = OnceLock::new();
    THEMES.get_or_init(|| {
        SOURCES
            .iter()
            .map(|(name, src)| {
                parse_colors(src)
                    .and_then(|c| derive_theme(name, &c))
                    .unwrap_or_else(|e| panic!("bundled theme {name}: {e}"))
            })
            .collect()
    })
}

/// Index of the theme named `name` (case-insensitive), or of the default theme.
pub fn theme_index(name: &str) -> usize {
    let find = |n: &str| themes().iter().position(|t| t.name.eq_ignore_ascii_case(n));
    find(name).or_else(|| find(DEFAULT_THEME)).unwrap_or(0)
}

/// Parses Omarchy's flat `key = "#rrggbb"` format.
pub fn parse_colors(src: &str) -> Result<HashMap<String, Rgb>, String> {
    let mut map = HashMap::new();
    for line in src
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("bad line: {line}"))?;
        let value = value.trim().trim_matches('"');
        map.insert(
            key.trim().to_string(),
            parse_hex(value).ok_or_else(|| format!("bad color: {line}"))?,
        );
    }
    Ok(map)
}

fn parse_hex(s: &str) -> Option<Rgb> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// Maps Omarchy color keys onto the app's roles (see the plan's Task 3 derivation rules).
pub fn derive_theme(name: &'static str, c: &HashMap<String, Rgb>) -> Result<Theme, String> {
    let get = |k: &str| {
        c.get(k)
            .copied()
            .ok_or_else(|| format!("{name}: missing {k}"))
    };
    let (bg, fg) = (get("background")?, get("foreground")?);

    Ok(Theme {
        name,
        bg,
        fg,
        dim: mix(fg, bg),
        accent: get("accent")?,
        selection: get("selection_background")?,
        error: get("color1")?,
        is_light: luminance(bg) > 0.5,
    })
}

fn mix(a: Rgb, b: Rgb) -> Rgb {
    let m = |x: u8, y: u8| ((x as u16 + y as u16) / 2) as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// WCAG relative luminance, 0.0 (black) ..= 1.0 (white).
pub fn luminance(c: Rgb) -> f32 {
    let lin = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2)
}

/// WCAG contrast ratio between two colors, lighter over darker, so always >= 1.
pub(crate) fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_palette() -> HashMap<String, Rgb> {
        let mut c = HashMap::new();
        for i in 0..16 {
            c.insert(format!("color{i}"), Rgb(0x80, 0x80, 0x80));
        }
        c.insert("background".into(), Rgb(0, 0, 0));
        c.insert("foreground".into(), Rgb(255, 255, 255));
        c.insert("accent".into(), Rgb(10, 10, 10));
        c.insert("selection_background".into(), Rgb(20, 20, 20));
        c
    }

    #[test]
    fn all_omarchy_themes_load() {
        let names: Vec<_> = themes().iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "catppuccin-latte",
                "catppuccin",
                "ethereal",
                "everforest",
                "flexoki-light",
                "gruvbox",
                "hackerman",
                "kanagawa",
                "lumon",
                "matte-black",
                "miasma",
                "nord",
                "osaka-jade",
                "retro-82",
                "ristretto",
                "rose-pine",
                "tokyo-night",
                "vantablack",
                "white"
            ]
        );
    }

    #[test]
    fn gruvbox_maps_exact_values() {
        let t = &themes()[theme_index("gruvbox")];
        assert_eq!(t.bg, Rgb(0x28, 0x28, 0x28));
        assert_eq!(t.fg, Rgb(0xd4, 0xbe, 0x98));
        assert_eq!(t.accent, Rgb(0x7d, 0xae, 0xa3));
        assert_eq!(t.selection, Rgb(0xd6, 0x5d, 0x0e));
    }

    #[test]
    fn missing_key_is_error() {
        let mut c = base_palette();
        c.remove("background");
        assert!(derive_theme("x", &c).is_err());
    }

    #[test]
    fn parse_colors_reads_flat_toml() {
        let c = parse_colors("# c\naccent = \"#82FB9C\"\n\nbackground = \"#0b0c16\"\n").unwrap();
        assert_eq!(c["accent"], Rgb(0x82, 0xfb, 0x9c));
        assert_eq!(c["background"], Rgb(0x0b, 0x0c, 0x16));
        assert!(parse_colors("accent = \"#zzz\"").is_err());
    }

    #[test]
    fn light_themes_detected() {
        for n in ["catppuccin-latte", "flexoki-light", "white"] {
            assert!(themes()[theme_index(n)].is_light, "{n}");
        }
        for n in ["tokyo-night", "gruvbox", "vantablack"] {
            assert!(!themes()[theme_index(n)].is_light, "{n}");
        }
    }

    #[test]
    fn block_fill_is_opaque_mix() {
        let t = &themes()[theme_index("gruvbox")];
        // Per channel (w * a + (1 - w) * b).round(): accent 7daea3 over 282828.
        assert_eq!(t.block_fill(Tone::Future), Rgb(0x5b, 0x78, 0x72));
        assert_eq!(t.block_fill(Tone::Past), Rgb(0x42, 0x50, 0x4d));
        assert_eq!(t.block_fill(Tone::Now), t.accent);
    }

    #[test]
    fn text_on_picks_contrast() {
        for t in themes() {
            for tone in [Tone::Past, Tone::Future, Tone::Now] {
                let f = t.block_fill(tone);
                let x = t.text_on(f);
                let other = if x == t.fg { t.bg } else { t.fg };
                assert!(contrast(x, f) >= contrast(other, f), "{} {tone:?}", t.name);
            }
        }
    }

    #[test]
    fn lookup_case_insensitive_with_fallback() {
        assert_eq!(themes()[theme_index("Rose-Pine")].name, "rose-pine");
        assert_eq!(themes()[theme_index("nope")].name, "tokyo-night");
    }
}
