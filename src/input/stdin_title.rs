use crate::title::{sanitize, truncate_end};

const MAX_DETECTED_TITLE_WIDTH: usize = 160;

pub(super) fn detect() -> Option<String> {
    detect_platform().and_then(|value| usable_title(&value))
}

pub(super) fn parse_cmdline(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.last() != Some(&0) {
        return None;
    }

    let mut arguments = bytes[..bytes.len() - 1].split(|byte| *byte == 0);
    let program = sanitize(&String::from_utf8_lossy(arguments.next()?));
    if program.is_empty() {
        return None;
    }
    let mut argv = vec![program];
    argv.extend(
        arguments
            .map(String::from_utf8_lossy)
            .map(|argument| sanitize(&argument))
            .filter(|argument| !argument.is_empty()),
    );

    usable_title(&argv.join(" "))
}

fn usable_title(value: &str) -> Option<String> {
    let sanitized = sanitize(value);
    (!sanitized.is_empty()).then(|| truncate_end(&sanitized, MAX_DETECTED_TITLE_WIDTH))
}

#[cfg(target_os = "linux")]
fn detect_platform() -> Option<String> {
    use std::{fs, path::PathBuf};

    let stdin_pipe = fs::read_link("/proc/self/fd/0").ok()?;
    let pipe_label = stdin_pipe.to_str()?;
    if !pipe_label.starts_with("pipe:[") || !pipe_label.ends_with(']') {
        return None;
    }

    let current_pid = std::process::id();
    let mut candidates = fs::read_dir("/proc")
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
            (pid != current_pid).then_some((pid, entry.path()))
        })
        .collect::<Vec<(u32, PathBuf)>>();
    candidates.sort_unstable_by_key(|(pid, _)| std::cmp::Reverse(*pid));

    candidates.into_iter().find_map(|(_, process_dir)| {
        let stdout = fs::read_link(process_dir.join("fd/1")).ok()?;
        if stdout != stdin_pipe {
            return None;
        }
        let cmdline = fs::read(process_dir.join("cmdline")).ok()?;
        parse_cmdline(&cmdline)
    })
}

#[cfg(not(target_os = "linux"))]
fn detect_platform() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use unicode_width::UnicodeWidthStr;

    use super::*;

    #[test]
    fn parses_and_joins_nul_separated_argv() {
        assert_eq!(
            parse_cmdline(b"journalctl\0-u\0ssh\0"),
            Some("journalctl -u ssh".to_string())
        );
    }

    #[test]
    fn sanitizes_command_arguments() {
        assert_eq!(
            parse_cmdline(b"docker\0logs\napp\0bad\x07arg\0"),
            Some("docker logs app bad arg".to_string())
        );
    }

    #[test]
    fn rejects_empty_and_malformed_cmdlines() {
        assert_eq!(parse_cmdline(b""), None);
        assert_eq!(parse_cmdline(b"\0"), None);
        assert_eq!(parse_cmdline(b"\0cat\0"), None);
        assert_eq!(parse_cmdline(b"cat\0README.md"), None);
    }

    #[test]
    fn caps_detected_command_display_width() {
        let mut bytes = vec![b'x'; 300];
        bytes.push(0);
        let title = parse_cmdline(&bytes).unwrap();

        assert!(UnicodeWidthStr::width(title.as_str()) <= MAX_DETECTED_TITLE_WIDTH);
        assert!(title.ends_with('…'));
    }
}
