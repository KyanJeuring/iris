use ratatui::style::Style;

use super::model::AlertKind;
use crate::theme::Theme;

pub fn label(kind: AlertKind, theme: &Theme) -> String {
    let (icon, label) = match kind {
        AlertKind::Note => (&theme.symbols.alert_note, "NOTE"),
        AlertKind::Tip => (&theme.symbols.alert_tip, "TIP"),
        AlertKind::Important => (&theme.symbols.alert_important, "IMPORTANT"),
        AlertKind::Warning => (&theme.symbols.alert_warning, "WARNING"),
        AlertKind::Caution => (&theme.symbols.alert_caution, "CAUTION"),
    };
    format!("{icon} {label}")
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
