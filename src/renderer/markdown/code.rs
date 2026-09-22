use std::sync::LazyLock;

use ratatui::{
    style::{Color, Modifier},
    text::Span,
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

use crate::theme::Theme;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub fn highlight(code: &str, language: Option<&str>, theme: &Theme) -> Vec<Vec<Span<'static>>> {
    let syntax = language
        .and_then(|token| {
            SYNTAX_SET
                .find_syntax_by_token(token)
                .or_else(|| SYNTAX_SET.find_syntax_by_extension(token))
        })
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let syntect_theme = THEME_SET
        .themes
        .get(&theme.syntax_theme)
        .or_else(|| THEME_SET.themes.get("base16-ocean.dark"))
        .or_else(|| THEME_SET.themes.values().next());

    let Some(syntect_theme) = syntect_theme else {
        return code
            .lines()
            .map(|line| vec![Span::styled(line.to_string(), theme.code)])
            .collect();
    };

    let mut highlighter = HighlightLines::new(syntax, syntect_theme);
    let mut lines = Vec::new();

    if code.is_empty() {
        lines.push(Vec::new());
        return lines;
    }

    for line in LinesWithEndings::from(code) {
        let text = line.strip_suffix('\n').unwrap_or(line);
        match highlighter.highlight_line(line, &SYNTAX_SET) {
            Ok(regions) => {
                let spans = regions
                    .into_iter()
                    .map(|(style, fragment)| {
                        let fragment = fragment.strip_suffix('\n').unwrap_or(fragment);
                        let mut ratatui_style = theme.code.fg(Color::Rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        ));
                        let mut modifiers = Modifier::empty();
                        if style.font_style.contains(FontStyle::BOLD) {
                            modifiers |= Modifier::BOLD;
                        }
                        if style.font_style.contains(FontStyle::ITALIC) {
                            modifiers |= Modifier::ITALIC;
                        }
                        if style.font_style.contains(FontStyle::UNDERLINE) {
                            modifiers |= Modifier::UNDERLINED;
                        }
                        ratatui_style = ratatui_style.add_modifier(modifiers);
                        Span::styled(fragment.to_string(), ratatui_style)
                    })
                    .collect::<Vec<_>>();
                if spans.is_empty() && !text.is_empty() {
                    lines.push(vec![Span::styled(text.to_string(), theme.code)]);
                } else {
                    lines.push(spans);
                }
            }
            Err(_) => lines.push(vec![Span::styled(text.to_string(), theme.code)]),
        }
    }

    lines
}
