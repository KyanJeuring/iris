use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub(crate) fn sanitize(value: &str) -> String {
    let mut output = String::new();
    let mut pending_space = false;

    for ch in value.chars() {
        if ch.is_control() || ch.is_whitespace() {
            pending_space = !output.is_empty();
        } else {
            if pending_space {
                output.push(' ');
                pending_space = false;
            }
            output.push(ch);
        }
    }

    output
}

pub(crate) fn truncate_end(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }

    const ELLIPSIS: &str = "…";
    let ellipsis_width = UnicodeWidthStr::width(ELLIPSIS);
    if max_width <= ellipsis_width {
        return ELLIPSIS.to_string();
    }

    let content_width = max_width - ellipsis_width;
    let mut output = String::new();
    let mut width = 0usize;
    for grapheme in value.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if width.saturating_add(grapheme_width) > content_width {
            break;
        }
        output.push_str(grapheme);
        width += grapheme_width;
    }
    output.push_str(ELLIPSIS);
    output
}

pub(crate) fn truncate_with_suffix(value: &str, suffix: &str, max_width: usize) -> String {
    let combined = format!("{value}{suffix}");
    if UnicodeWidthStr::width(combined.as_str()) <= max_width {
        return combined;
    }

    let suffix_width = UnicodeWidthStr::width(suffix);
    if suffix_width >= max_width {
        return truncate_end(suffix.trim_start(), max_width);
    }

    format!(
        "{}{}",
        truncate_end(value, max_width - suffix_width),
        suffix
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_controls_and_collapses_whitespace() {
        assert_eq!(
            sanitize("  journalctl\n-u\tssh\u{7}  "),
            "journalctl -u ssh"
        );
        assert_eq!(sanitize("\0\n\t"), "");
    }

    #[test]
    fn truncates_on_grapheme_and_display_width_boundaries() {
        let value = "日志🙂service";
        let truncated = truncate_end(value, 7);

        assert_eq!(truncated, "日志🙂…");
        assert!(UnicodeWidthStr::width(truncated.as_str()) <= 7);
    }

    #[test]
    fn truncates_a_title_without_losing_its_zoom_suffix() {
        let truncated = truncate_with_suffix("非常に長い画像名.png", " [150%]", 17);

        assert!(truncated.ends_with(" [150%]"));
        assert!(UnicodeWidthStr::width(truncated.as_str()) <= 17);
    }
}
