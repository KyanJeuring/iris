use ratatui::style::Style;

use super::model::AlertKind;
use crate::theme::Theme;

pub fn icon(kind: AlertKind, theme: &Theme) -> &str {
    match kind {
        AlertKind::Note => &theme.symbols.alert_note,
        AlertKind::Tip => &theme.symbols.alert_tip,
        AlertKind::Important => &theme.symbols.alert_important,
        AlertKind::Warning => &theme.symbols.alert_warning,
        AlertKind::Caution => &theme.symbols.alert_caution,
    }
}

pub fn label(kind: AlertKind, theme: &Theme) -> String {
    let label = match kind {
        AlertKind::Note => "NOTE",
        AlertKind::Tip => "TIP",
        AlertKind::Important => "IMPORTANT",
        AlertKind::Warning => "WARNING",
        AlertKind::Caution => "CAUTION",
    };
    format!("{} {label}", icon(kind, theme))
}

pub fn style(kind: AlertKind, theme: &Theme) -> Style {
    match kind {
        AlertKind::Note => theme.alert_note,
        AlertKind::Tip => theme.alert_tip,
        AlertKind::Important => theme.alert_important,
        AlertKind::Warning => theme.alert_warning,
        AlertKind::Caution => theme.alert_caution,
    }
}
