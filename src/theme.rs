use ratatui::style::{Color, Style, Stylize};
use std::{env, fs, path::PathBuf, sync::LazyLock, time::Duration};
use toml::{Table, Value};

static PRESETS: LazyLock<Table> = LazyLock::new(|| {
    include_str!("theme-presets.toml")
        .parse()
        .expect("valid bundled Herdr theme presets")
});

#[derive(Clone, Copy)]
pub struct Palette {
    pub title: Style,
    pub subtitle: Style,
    pub inactive_title: Style,
    pub inactive_subtitle: Style,
    pub panel: Style,
    pub primary: Style,
    pub secondary: Style,
    pub accent: Color,
    pub warning: Color,
}

fn canonical(name: &str) -> String {
    let name = name.to_lowercase().replace([' ', '_'], "-");
    match name.as_str() {
        "catppuccin-mocha" => "catppuccin",
        "latte" | "light" => "catppuccin-latte",
        "tokyonight" => "tokyo-night",
        "tokyo-day" | "tokyonight-day" => "tokyo-night-day",
        "gruvbox-dark" => "gruvbox",
        "onedark" => "one-dark",
        "onelight" => "one-light",
        "solarized-dark" => "solarized",
        "lotus" => "kanagawa-lotus",
        "rosepine" => "rose-pine",
        "dawn" | "rosepine-dawn" => "rose-pine-dawn",
        _ => return name,
    }
    .into()
}

fn sibling(name: &str, light: bool) -> &str {
    for (dark, day) in [
        ("catppuccin", "catppuccin-latte"),
        ("tokyo-night", "tokyo-night-day"),
        ("gruvbox", "gruvbox-light"),
        ("one-dark", "one-light"),
        ("solarized", "solarized-light"),
        ("kanagawa", "kanagawa-lotus"),
        ("rose-pine", "rose-pine-dawn"),
    ] {
        if name == dark || name == day {
            return if light { day } else { dark };
        }
    }
    name
}

fn color(value: &str) -> Color {
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value
        .strip_prefix('#')
        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        let mut c = hex.chars().map(|n| n.to_digit(16).unwrap() as u8 * 17);
        return Color::Rgb(c.next().unwrap(), c.next().unwrap(), c.next().unwrap());
    }
    if let Some(rgb) = value.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let mut c = rgb.split(',').map(|n| n.trim().parse::<u8>());
        if let (Some(Ok(r)), Some(Ok(g)), Some(Ok(b)), None) =
            (c.next(), c.next(), c.next(), c.next())
        {
            return Color::Rgb(r, g, b);
        }
    }
    match value.as_str() {
        "none" | "transparent" | "default" => Color::Reset,
        "purple" => Color::Magenta,
        _ => value.parse().unwrap_or(Color::Cyan),
    }
}

impl Palette {
    fn resolve(doc: &Table, light: bool) -> Self {
        let theme = doc.get("theme");
        let field = |k: &str| theme?.get(k)?.as_str();
        let auto = theme
            .and_then(|t| t.get("auto_switch"))
            .and_then(Value::as_bool)
            == Some(true);
        let manual = canonical(field("name").unwrap_or("catppuccin"));
        let mode = if light { "light" } else { "dark" };
        let name = if auto {
            canonical(
                field(if light { "light_name" } else { "dark_name" })
                    .unwrap_or(sibling(&manual, light)),
            )
        } else {
            manual
        };
        let fallback = if auto && light {
            "catppuccin-latte"
        } else {
            "catppuccin"
        };
        let preset = PRESETS.get(&name).unwrap_or(&PRESETS[fallback]);
        let custom = theme.and_then(|t| t.get("custom"));
        let token = |k: &str| {
            let scoped = if auto {
                custom.and_then(|c| c.get(mode)).and_then(|c| c.get(k))
            } else {
                None
            };
            let legacy = if k == "accent" {
                doc.get("ui")
                    .and_then(|v| v.get("accent"))
                    .filter(|v| v.as_str() != Some("cyan"))
            } else {
                None
            };
            color(
                scoped
                    .or_else(|| custom.and_then(|c| c.get(k)))
                    .or(legacy)
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| preset[k].as_str().unwrap()),
            )
        };
        let selection = match token("selection_bg") {
            Color::Reset => token("active_row_bg"),
            c => c,
        };
        let on_accent = match token("panel_bg") {
            Color::Reset => token("surface_dim"),
            c => c,
        };
        Self {
            title: Style::new().fg(token("text")).bg(selection).bold(),
            subtitle: Style::new().fg(token("mauve")).bg(selection),
            inactive_title: Style::new().fg(token("subtext0")),
            inactive_subtitle: Style::new().fg(token("overlay0")),
            panel: Style::new().fg(token("text")).bg(token("panel_bg")),
            primary: Style::new().fg(on_accent).bg(token("accent")).bold(),
            secondary: Style::new().fg(token("text")).bg(token("surface0")).bold(),
            accent: token("accent"),
            warning: token("yellow"),
        }
    }
}

pub struct Theme {
    path: Option<PathBuf>,
    contents: String,
    light: bool,
    pub palette: Palette,
}

impl Theme {
    pub fn load() -> Self {
        let path = crate::bindings::config_path().ok();
        let contents = path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default();
        let doc = contents
            .trim_start_matches('\u{feff}')
            .parse::<Table>()
            .unwrap_or_default();
        let mut options = terminal_colorsaurus::QueryOptions::default();
        options.timeout = Duration::from_millis(200);
        // Sample before crossterm starts consuming input. This also allows a saved
        // config change to turn auto_switch on while the shelf is running.
        let light = terminal_colorsaurus::background_color(options)
            .map(|c| {
                let (r, g, b) = c.scale_to_8bit();
                u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 >= 128000
            })
            .unwrap_or_else(|_| {
                env::var("COLORFGBG")
                    .ok()
                    .and_then(|s| s.rsplit(';').next()?.parse::<u8>().ok())
                    .is_some_and(|n| n == 7 || (9..=15).contains(&n))
            });
        Self {
            path,
            contents,
            light,
            palette: Palette::resolve(&doc, light),
        }
    }
    pub fn refresh(&mut self) {
        let Some(path) = &self.path else { return };
        let contents = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(_) => return,
        };
        if contents != self.contents {
            if let Ok(doc) = contents.trim_start_matches('\u{feff}').parse::<Table>() {
                self.palette = Palette::resolve(&doc, self.light);
            }
            self.contents = contents;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_workspace_and_button_tokens_follow_the_theme() {
        let doc = r##"
        [theme]
        name = "tokyo-night"
        auto_switch = true
        [theme.custom]
        text = "#ace"
        selection_bg = "reset"
        active_row_bg = "rgb(1, 2, 3)"
        surface0 = "#123456"
        [theme.custom.light]
        accent = "#345"
        "##
        .parse::<Table>()
        .unwrap();
        let dark = Palette::resolve(&doc, false);
        let day = Palette::resolve(&doc, true);
        assert_eq!(dark.title.bg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(dark.subtitle.bg, dark.title.bg);
        assert_eq!(dark.title.fg, Some(Color::Rgb(170, 204, 238)));
        assert_ne!(dark.subtitle.fg, day.subtitle.fg);
        assert_eq!(day.primary.bg, Some(Color::Rgb(51, 68, 85)));
        assert_eq!(day.secondary.bg, Some(Color::Rgb(18, 52, 86)));
    }
}
