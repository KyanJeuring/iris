use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

use crate::options::IconMode;

const EMBER_TOML: &str = include_str!("../themes/ember.toml");

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
    pub status: Style,
    pub status_accent: Style,
    pub help: Style,
    pub help_border: Style,
    pub help_heading: Style,
    pub help_key: Style,
}

impl Theme {
    pub fn ember(icon_mode: IconMode) -> Result<Self> {
        Self::from_toml(EMBER_TOML, "built-in Ember theme", icon_mode)
    }

    pub fn load(name_or_path: &str, icon_mode: IconMode) -> Result<Self> {
        if name_or_path.eq_ignore_ascii_case("ember") {
            return Self::ember(icon_mode);
        }

        let direct = Path::new(name_or_path);
        if direct.is_file() {
            let source = fs::read_to_string(direct)
                .with_context(|| format!("failed to read theme '{}'", direct.display()))?;
            return Self::from_toml(&source, &direct.display().to_string(), icon_mode);
        }

        let path = themes_dir()
            .map(|dir| dir.join(format!("{name_or_path}.toml")))
            .ok_or_else(|| anyhow::anyhow!("could not determine the user config directory"))?;

        if !path.is_file() {
            bail!(
                "unknown theme '{name_or_path}' (expected '{}' or a theme file path)",
                path.display()
            );
        }

        let source = fs::read_to_string(&path)
            .with_context(|| format!("failed to read theme '{}'", path.display()))?;
        Self::from_toml(&source, &path.display().to_string(), icon_mode)
    }

    pub fn list_available() -> Vec<String> {
        let mut themes = vec!["ember (built-in)".to_string()];
        if let Some(dir) = themes_dir()
            && let Ok(entries) = fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|v| v.to_str()) == Some("toml")
                    && let Some(name) = path.file_stem().and_then(|v| v.to_str())
                    && !name.eq_ignore_ascii_case("ember")
                {
                    themes.push(name.to_string());
                }
            }
        }
        themes.sort();
        themes
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

fn default_image_height() -> u16 {
    14
}

pub fn themes_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("iris").join("themes"))
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
    use super::*;

    #[test]
    fn built_in_ember_loads() {
        let theme = Theme::ember(IconMode::NerdFont).unwrap();
        assert_eq!(theme.name, "Ember");
        assert_eq!(theme.heading.styles.len(), 6);
        assert_eq!(theme.symbols.task_checked, "󰄲");
    }

    #[test]
    fn built_in_ember_has_unicode_fallback_symbols() {
        let theme = Theme::ember(IconMode::Unicode).unwrap();
        assert_eq!(theme.symbols.task_checked, "☑");
        assert_eq!(theme.symbols.alert_warning, "⚠");
    }
}
