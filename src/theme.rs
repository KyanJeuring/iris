use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

use crate::options::IconMode;

const DEFAULT_TOML: &str = include_str!("../themes/default.toml");
const EMBER_TOML: &str = include_str!("../themes/ember.toml");
const NORD_TOML: &str = include_str!("../themes/nord.toml");
const GRUVBOX_TOML: &str = include_str!("../themes/gruvbox.toml");
const DRACULA_TOML: &str = include_str!("../themes/dracula.toml");
const CATPPUCCIN_MOCHA_TOML: &str = include_str!("../themes/catppuccin-mocha.toml");
const TOKYO_NIGHT_TOML: &str = include_str!("../themes/tokyo-night.toml");
const ROSE_PINE_TOML: &str = include_str!("../themes/rose-pine.toml");
const KANAGAWA_TOML: &str = include_str!("../themes/kanagawa.toml");
const EVERFOREST_DARK_TOML: &str = include_str!("../themes/everforest-dark.toml");
const ONE_DARK_TOML: &str = include_str!("../themes/one-dark.toml");
const CATPPUCCIN_LATTE_TOML: &str = include_str!("../themes/catppuccin-latte.toml");
const GRUVBOX_LIGHT_TOML: &str = include_str!("../themes/gruvbox-light.toml");
const ROSE_PINE_DAWN_TOML: &str = include_str!("../themes/rose-pine-dawn.toml");
const SOLARIZED_LIGHT_TOML: &str = include_str!("../themes/solarized-light.toml");
const EVERFOREST_LIGHT_TOML: &str = include_str!("../themes/everforest-light.toml");
const MATRIX_TOML: &str = include_str!("../themes/matrix.toml");
const AMBER_CRT_TOML: &str = include_str!("../themes/amber-crt.toml");
const VAPORWAVE_TOML: &str = include_str!("../themes/vaporwave.toml");
const RETRO_BLUE_TOML: &str = include_str!("../themes/retro-blue.toml");
const HOT_DOG_STAND_TOML: &str = include_str!("../themes/hot-dog-stand.toml");

const BUNDLED_THEMES: &[(&str, &str)] = &[
    ("ember", EMBER_TOML),
    ("nord", NORD_TOML),
    ("gruvbox", GRUVBOX_TOML),
    ("dracula", DRACULA_TOML),
    ("catppuccin-mocha", CATPPUCCIN_MOCHA_TOML),
    ("tokyo-night", TOKYO_NIGHT_TOML),
    ("rose-pine", ROSE_PINE_TOML),
    ("kanagawa", KANAGAWA_TOML),
    ("everforest-dark", EVERFOREST_DARK_TOML),
    ("one-dark", ONE_DARK_TOML),
    ("catppuccin-latte", CATPPUCCIN_LATTE_TOML),
    ("gruvbox-light", GRUVBOX_LIGHT_TOML),
    ("rose-pine-dawn", ROSE_PINE_DAWN_TOML),
    ("solarized-light", SOLARIZED_LIGHT_TOML),
    ("everforest-light", EVERFOREST_LIGHT_TOML),
    ("matrix", MATRIX_TOML),
    ("amber-crt", AMBER_CRT_TOML),
    ("vaporwave", VAPORWAVE_TOML),
    ("retro-blue", RETRO_BLUE_TOML),
    ("hot-dog-stand", HOT_DOG_STAND_TOML),
];

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StyleSpec {
    pub foreground: Option<String>,
    pub background: Option<String>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub strikethrough: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct HeadingStyleSpec {
    #[serde(flatten)]
    style: StyleSpec,
    separator: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct HeadingSection {
    h1: HeadingStyleSpec,
    h2: HeadingStyleSpec,
    h3: HeadingStyleSpec,
    h4: HeadingStyleSpec,
    h5: HeadingStyleSpec,
    h6: HeadingStyleSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct ListSpec {
    #[serde(flatten)]
    style: StyleSpec,
    marker_foreground: Option<String>,
    bullet: String,
    indent: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct QuoteSpec {
    #[serde(flatten)]
    style: StyleSpec,
    border_foreground: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CodeSpec {
    #[serde(flatten)]
    style: StyleSpec,
    border_foreground: Option<String>,
    syntax_theme: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TableSpec {
    #[serde(flatten)]
    style: StyleSpec,
    border_foreground: Option<String>,
    header_foreground: Option<String>,
    #[serde(default)]
    header_bold: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskSection {
    checked: StyleSpec,
    unchecked: StyleSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct ImageSpec {
    #[serde(flatten)]
    style: StyleSpec,
    #[serde(default = "default_image_height")]
    max_height: u16,
}

#[derive(Debug, Clone, Deserialize)]
struct HtmlSection {
    mark: StyleSpec,
    kbd: StyleSpec,
    underline: StyleSpec,
    subtle: StyleSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct SymbolSetSpec {
    task_checked: String,
    task_unchecked: String,
    alert_note: String,
    alert_tip: String,
    alert_important: String,
    alert_warning: String,
    alert_caution: String,
    image: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SymbolSection {
    nerd_font: SymbolSetSpec,
    unicode: SymbolSetSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct AlertSection {
    note: StyleSpec,
    tip: StyleSpec,
    important: StyleSpec,
    warning: StyleSpec,
    caution: StyleSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct SearchSection {
    #[serde(rename = "match")]
    match_style: StyleSpec,
    current: StyleSpec,
    prompt: StyleSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct UiSection {
    #[serde(default)]
    border: Option<StyleSpec>,
    status: StyleSpec,
    status_accent: StyleSpec,
    help: StyleSpec,
    help_border: StyleSpec,
    help_heading: StyleSpec,
    help_key: StyleSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct TextSection {
    strong: StyleSpec,
    emphasis: StyleSpec,
    strikethrough: StyleSpec,
}

#[derive(Debug, Clone, Deserialize)]
struct ThemeFile {
    name: String,
    palette: HashMap<String, String>,
    document: StyleSpec,
    heading: HeadingSection,
    text: TextSection,
    inline_code: StyleSpec,
    link: StyleSpec,
    list: ListSpec,
    quote: QuoteSpec,
    code: CodeSpec,
    table: TableSpec,
    horizontal_rule: StyleSpec,
    task: TaskSection,
    image: ImageSpec,
    html: HtmlSection,
    symbols: SymbolSection,
    alert: AlertSection,
    search: SearchSection,
    ui: UiSection,
}

#[derive(Debug, Clone)]
pub struct HeadingTheme {
    pub styles: [Style; 6],
    pub separators: [Option<String>; 6],
}

#[derive(Debug, Clone)]
pub struct ListTheme {
    pub style: Style,
    pub marker_style: Style,
    pub bullet: String,
    pub indent: usize,
}

#[derive(Debug, Clone)]
pub struct SymbolTheme {
    pub task_checked: String,
    pub task_unchecked: String,
    pub alert_note: String,
    pub alert_tip: String,
    pub alert_important: String,
    pub alert_warning: String,
    pub alert_caution: String,
    pub image: String,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub document: Style,
    pub heading: HeadingTheme,
    pub strong: Style,
    pub emphasis: Style,
    pub strikethrough: Style,
    pub inline_code: Style,
    pub link: Style,
    pub list: ListTheme,
    pub quote: Style,
    pub quote_border: Style,
    pub code: Style,
    pub code_border: Style,
    pub syntax_theme: String,
    pub table: Style,
    pub table_border: Style,
    pub table_header: Style,
    pub horizontal_rule: Style,
    pub task_checked: Style,
    pub task_unchecked: Style,
    pub image: Style,
    pub image_max_height: u16,
    pub html_mark: Style,
    pub html_kbd: Style,
    pub html_underline: Style,
    pub html_subtle: Style,
    pub symbols: SymbolTheme,
    pub alert_note: Style,
    pub alert_tip: Style,
    pub alert_important: Style,
    pub alert_warning: Style,
    pub alert_caution: Style,
    pub search_match: Style,
    pub search_current: Style,
    pub search_prompt: Style,
    pub border: Style,
    pub status: Style,
    pub status_accent: Style,
    pub help: Style,
    pub help_border: Style,
    pub help_heading: Style,
    pub help_key: Style,
}

impl Theme {
    pub fn builtin_default(icon_mode: IconMode) -> Result<Self> {
        Self::from_toml(DEFAULT_TOML, "built-in default theme", icon_mode)
    }

    pub fn ember(icon_mode: IconMode) -> Result<Self> {
        Self::from_toml(EMBER_TOML, "bundled Ember theme", icon_mode)
    }

    pub fn load(name_or_path: &str, icon_mode: IconMode) -> Result<Self> {
        if name_or_path.eq_ignore_ascii_case("default") {
            return Self::builtin_default(icon_mode);
        }

        let direct = Path::new(name_or_path);
        if direct.is_file() {
            let source = fs::read_to_string(direct)
                .with_context(|| format!("failed to read theme '{}'", direct.display()))?;
            return Self::from_toml(&source, &direct.display().to_string(), icon_mode);
        }

        let normalized = name_or_path.to_ascii_lowercase();
        let file_name = if bundled_theme_source(&normalized).is_some() {
            &normalized
        } else {
            name_or_path
        };

        let path = themes_dir()
            .map(|dir| dir.join(format!("{file_name}.toml")))
            .ok_or_else(|| anyhow::anyhow!("could not determine the user config directory"))?;

        if path.is_file() {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read theme '{}'", path.display()))?;
            return Self::from_toml(&source, &path.display().to_string(), icon_mode);
        }

        if let Some(source) = bundled_theme_source(&normalized) {
            return Self::from_toml(
                source,
                &format!("bundled {normalized} theme fallback"),
                icon_mode,
            );
        }

        bail!(
            "unknown theme '{name_or_path}' (expected '{}' or a theme file path)",
            path.display()
        );
    }

    pub fn list_available() -> Vec<String> {
        let mut themes = BTreeSet::new();

        for (name, _) in BUNDLED_THEMES {
            themes.insert((*name).to_string());
        }

        if let Some(dir) = themes_dir()
            && let Ok(entries) = fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                    continue;
                }

                let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
                    continue;
                };

                if !name.eq_ignore_ascii_case("default") {
                    themes.insert(name.to_string());
                }
            }
        }

        let mut available = vec!["default (built-in)".to_string()];
        available.extend(themes);
        available
    }

    fn from_toml(source: &str, label: &str, icon_mode: IconMode) -> Result<Self> {
        let file: ThemeFile = toml::from_str(source)
            .with_context(|| format!("invalid theme configuration in {label}"))?;
        let palette = &file.palette;

        let heading_specs = [
            &file.heading.h1,
            &file.heading.h2,
            &file.heading.h3,
            &file.heading.h4,
            &file.heading.h5,
            &file.heading.h6,
        ];
        let styles_vec = heading_specs
            .iter()
            .map(|spec| resolve_style(&spec.style, palette))
            .collect::<Result<Vec<_>>>()?;
        let styles: [Style; 6] = styles_vec
            .try_into()
            .map_err(|_| anyhow::anyhow!("theme must define six heading styles"))?;
        let separators = heading_specs.map(|spec| spec.separator.clone());

        let list_style = resolve_style(&file.list.style, palette)?;
        let marker_style =
            style_with_fg(list_style, file.list.marker_foreground.as_deref(), palette)?;
        let quote_style = resolve_style(&file.quote.style, palette)?;
        let quote_border = style_with_fg(
            Style::default(),
            file.quote.border_foreground.as_deref(),
            palette,
        )?;
        let code_style = resolve_style(&file.code.style, palette)?;
        let code_border = style_with_fg(
            Style::default(),
            file.code.border_foreground.as_deref(),
            palette,
        )?;
        let table_style = resolve_style(&file.table.style, palette)?;
        let mut table_header = style_with_fg(
            table_style,
            file.table.header_foreground.as_deref(),
            palette,
        )?;
        if file.table.header_bold {
            table_header = table_header.add_modifier(Modifier::BOLD);
        }

        let symbols = match icon_mode {
            IconMode::NerdFont => &file.symbols.nerd_font,
            IconMode::Unicode => &file.symbols.unicode,
        };

        Ok(Self {
            name: file.name,
            document: resolve_style(&file.document, palette)?,
            heading: HeadingTheme { styles, separators },
            strong: resolve_style(&file.text.strong, palette)?,
            emphasis: resolve_style(&file.text.emphasis, palette)?,
            strikethrough: resolve_style(&file.text.strikethrough, palette)?,
            inline_code: resolve_style(&file.inline_code, palette)?,
            link: resolve_style(&file.link, palette)?,
            list: ListTheme {
                style: list_style,
                marker_style,
                bullet: file.list.bullet,
                indent: file.list.indent.max(1),
            },
            quote: quote_style,
            quote_border,
            code: code_style,
            code_border,
            syntax_theme: file.code.syntax_theme,
            table: table_style,
            table_border: style_with_fg(
                Style::default(),
                file.table.border_foreground.as_deref(),
                palette,
            )?,
            table_header,
            horizontal_rule: resolve_style(&file.horizontal_rule, palette)?,
            task_checked: resolve_style(&file.task.checked, palette)?,
            task_unchecked: resolve_style(&file.task.unchecked, palette)?,
            image: resolve_style(&file.image.style, palette)?,
            image_max_height: file.image.max_height.max(1),
            html_mark: resolve_style(&file.html.mark, palette)?,
            html_kbd: resolve_style(&file.html.kbd, palette)?,
            html_underline: resolve_style(&file.html.underline, palette)?,
            html_subtle: resolve_style(&file.html.subtle, palette)?,
            symbols: SymbolTheme {
                task_checked: symbols.task_checked.clone(),
                task_unchecked: symbols.task_unchecked.clone(),
                alert_note: symbols.alert_note.clone(),
                alert_tip: symbols.alert_tip.clone(),
                alert_important: symbols.alert_important.clone(),
                alert_warning: symbols.alert_warning.clone(),
                alert_caution: symbols.alert_caution.clone(),
                image: symbols.image.clone(),
            },
            alert_note: resolve_style(&file.alert.note, palette)?,
            alert_tip: resolve_style(&file.alert.tip, palette)?,
            alert_important: resolve_style(&file.alert.important, palette)?,
            alert_warning: resolve_style(&file.alert.warning, palette)?,
            alert_caution: resolve_style(&file.alert.caution, palette)?,
            search_match: resolve_style(&file.search.match_style, palette)?,
            search_current: resolve_style(&file.search.current, palette)?,
            search_prompt: resolve_style(&file.search.prompt, palette)?,
            border: match &file.ui.border {
                Some(border) => resolve_style(border, palette)?,
                None => resolve_style(&file.ui.status_accent, palette)?,
            },
            status: resolve_style(&file.ui.status, palette)?,
            status_accent: resolve_style(&file.ui.status_accent, palette)?,
            help: resolve_style(&file.ui.help, palette)?,
            help_border: resolve_style(&file.ui.help_border, palette)?,
            help_heading: resolve_style(&file.ui.help_heading, palette)?,
            help_key: resolve_style(&file.ui.help_key, palette)?,
        })
    }

    pub fn heading_style(&self, level: u8) -> Style {
        self.heading.styles[level.clamp(1, 6) as usize - 1]
    }

    pub fn heading_separator(&self, level: u8) -> Option<&str> {
        self.heading.separators[level.clamp(1, 6) as usize - 1].as_deref()
    }
}

pub fn ensure_bundled_themes() {
    let Some(dir) = themes_dir() else {
        return;
    };

    if let Err(error) = fs::create_dir_all(&dir) {
        eprintln!(
            "iris: warning: could not create theme directory '{}': {error}",
            dir.display()
        );
        return;
    }

    for (name, source) in BUNDLED_THEMES {
        if let Err(error) = install_bundled_theme(&dir, name, source) {
            eprintln!(
                "iris: warning: could not install bundled theme '{}': {error}",
                dir.join(format!("{name}.toml")).display()
            );
        }
    }
}

fn install_bundled_theme(dir: &Path, name: &str, source: &str) -> std::io::Result<bool> {
    let path = dir.join(format!("{name}.toml"));

    let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };

    if let Err(error) = file.write_all(source.as_bytes()) {
        let _ = fs::remove_file(&path);
        return Err(error);
    }

    Ok(true)
}

fn bundled_theme_source(name: &str) -> Option<&'static str> {
    BUNDLED_THEMES
        .iter()
        .find_map(|(bundled_name, source)| (*bundled_name == name).then_some(*source))
}

fn default_image_height() -> u16 {
    14
}

pub fn themes_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|path| path.join("iris").join("themes"))
}

fn resolve_style(spec: &StyleSpec, palette: &HashMap<String, String>) -> Result<Style> {
    let mut style = Style::default();
    if let Some(value) = &spec.foreground {
        style = style.fg(resolve_color(value, palette)?);
    }
    if let Some(value) = &spec.background {
        style = style.bg(resolve_color(value, palette)?);
    }

    let mut modifiers = Modifier::empty();
    if spec.bold {
        modifiers |= Modifier::BOLD;
    }
    if spec.italic {
        modifiers |= Modifier::ITALIC;
    }
    if spec.underline {
        modifiers |= Modifier::UNDERLINED;
    }
    if spec.strikethrough {
        modifiers |= Modifier::CROSSED_OUT;
    }
    Ok(style.add_modifier(modifiers))
}

fn style_with_fg(
    mut style: Style,
    value: Option<&str>,
    palette: &HashMap<String, String>,
) -> Result<Style> {
    if let Some(value) = value {
        style = style.fg(resolve_color(value, palette)?);
    }
    Ok(style)
}

fn resolve_color(value: &str, palette: &HashMap<String, String>) -> Result<Color> {
    let resolved = palette.get(value).map(String::as_str).unwrap_or(value);
    parse_color(resolved).with_context(|| format!("unknown color '{value}'"))
}

fn parse_color(value: &str) -> Result<Color> {
    if let Some(hex) = value.strip_prefix('#')
        && hex.len() == 6
    {
        let r = u8::from_str_radix(&hex[0..2], 16)?;
        let g = u8::from_str_radix(&hex[2..4], 16)?;
        let b = u8::from_str_radix(&hex[4..6], 16)?;
        return Ok(Color::Rgb(r, g, b));
    }

    let color = match value.to_ascii_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "white" => Color::White,
        "default" | "reset" => Color::Reset,
        other => bail!("invalid color '{other}'"),
    };
    Ok(color)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_theme_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("iris-{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn built_in_default_loads() {
        let theme = Theme::builtin_default(IconMode::NerdFont).unwrap();
        assert_eq!(theme.name, "Default");
        assert_eq!(theme.heading.styles.len(), 6);
        assert_eq!(theme.symbols.task_checked, "󰄲");
    }

    #[test]
    fn built_in_default_has_unicode_fallback_symbols() {
        let theme = Theme::builtin_default(IconMode::Unicode).unwrap();
        assert_eq!(theme.symbols.task_checked, "☑");
        assert_eq!(theme.symbols.alert_warning, "⚠");
    }

    #[test]
    fn bundled_themes_all_parse() {
        for (name, source) in BUNDLED_THEMES {
            Theme::from_toml(source, name, IconMode::Unicode)
                .unwrap_or_else(|error| panic!("{name} should parse: {error:#}"));
        }
    }

    #[test]
    fn installing_bundled_theme_does_not_overwrite_existing_file() {
        let dir = temp_theme_dir("preserve-theme");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ember.toml");
        fs::write(&path, "custom ember contents").unwrap();

        let installed = install_bundled_theme(&dir, "ember", EMBER_TOML).unwrap();

        assert!(!installed);
        assert_eq!(fs::read_to_string(&path).unwrap(), "custom ember contents");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn installing_bundled_theme_creates_missing_file() {
        let dir = temp_theme_dir("install-theme");
        fs::create_dir_all(&dir).unwrap();

        let installed = install_bundled_theme(&dir, "ember", EMBER_TOML).unwrap();
        let path = dir.join("ember.toml");

        assert!(installed);
        assert_eq!(fs::read_to_string(&path).unwrap(), EMBER_TOML);
        fs::remove_dir_all(&dir).unwrap();
    }
}
