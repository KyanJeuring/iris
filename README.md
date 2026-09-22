# IRIS (Interactive Renderer for Information Sources)

### Terminal-native viewing for documents, data, and everything in between

IRIS is a general-purpose terminal viewer for rendering and exploring different kinds of files and structured data from one interface.

Instead of switching between separate tools for Markdown, JSON, CSV files, logs, images, PDFs, databases, and other formats, IRIS aims to provide a single terminal-native way to inspect them.

Think of it as a more visual and extensible `less` for modern files and data.

## What can IRIS view?

IRIS is designed to support formats such as:

* Markdown
* JSON
* YAML
* TOML
* CSV
* Images
* PDFs
* SQLite databases
* Logs and plain text
* Standard input

The current release focuses on plain text and Markdown. Markdown rendering includes headings, lists and task lists, tables, blockquotes and GitHub-style alerts, syntax-highlighted code blocks, links, embedded HTML, and Markdown images. Additional format-specific renderers will be added incrementally.

## Installation

### Cargo

Install the latest published release from crates.io:

```bash
cargo install iris-reader --locked
```

The crate is named `iris-reader`, but the installed command is simply:

```bash
iris
```

Cargo installs binaries into `~/.cargo/bin` by default, so make sure that directory is on your `PATH`.

### Prebuilt GitHub release

Download the archive for your platform from the GitHub Releases page, extract it, and place the `iris` binary somewhere on your `PATH`.

On Linux and macOS, a local installation can use:

```bash
install -Dm755 iris ~/.local/bin/iris
```

Windows releases contain `iris.exe`; place it in a directory that is included in your `PATH`.

### From source

IRIS requires Rust 1.94 or newer when building from source.

```bash
git clone https://github.com/KyanJeuring/iris.git
cd iris
make install
```

`make install` builds an optimized release binary and installs it to `~/.local/bin/iris` by default.

## Updating

How IRIS should be updated depends on how it was installed.

### Cargo

```bash
cargo install iris-reader --locked --force
```

### Prebuilt GitHub release

Download the newer release for your platform and replace the existing `iris` / `iris.exe` binary.

### Source installation

From the cloned repository:

```bash
make update
```

This performs a fast-forward-only `git pull`, rebuilds IRIS in release mode, and reinstalls the binary.

## Uninstalling

### Cargo

```bash
cargo uninstall iris-reader
```

### Prebuilt or local installation

If the binary was installed to `~/.local/bin`:

```bash
rm -f ~/.local/bin/iris
```

For a source installation, the same can be done through the Makefile:

```bash
make uninstall
```

Support for additional package managers such as APT, Pacman/AUR, Homebrew, and WinGet is planned.

## Usage

Open a file directly:

```bash
iris README.md
iris notes.txt
```

Or read standard input:

```bash
journalctl | iris
cat README.md | iris --format markdown
```

IRIS detects Markdown from file extensions. For piped input, use `--format markdown` when Markdown rendering is desired.

Useful options include:

```bash
iris --help
iris --version
iris --theme ember README.md
iris --icons unicode README.md
iris --list-themes
```

Ember is the built-in default theme and uses Nerd Font icons by default. If the terminal does not use a Nerd Font, switch to the portable Unicode icon set with `--icons unicode` or set `icons = "unicode"` in `~/.config/iris/config.toml`.

## Navigation

Inside the viewer:

```text
j / k, arrows       Scroll vertically
h / l, arrows       Scroll horizontally
PgUp / PgDn         Scroll by page
g / G               Top / bottom
[h / ]h             Previous / next Markdown heading
/                    Search
n / N                Next / previous search result
Esc                  Clear search / close an overlay
w                    Toggle wrapping
?                    Help
q                    Quit
```

Mouse-wheel and trackpad scrolling are supported. Shift + wheel scrolls horizontally. Markdown links can be opened by clicking them.

## Markdown images and HTML

Markdown images are rendered as terminal image blocks, including images placed inside normal paragraph flow. Standalone HTML `<img>` tags are rendered the same way. More complex inline HTML/image combinations fall back to a styled clickable placeholder so the surrounding text stays readable. IRIS detects terminal graphics capabilities and can use Kitty, Sixel, or iTerm2 image protocols, with a terminal-cell fallback when needed.

Local image paths are resolved relative to the Markdown document. Remote HTTP(S) images are also supported. Common raster formats include PNG, JPEG, GIF, WebP, BMP, and ICO, and SVG images are rasterized automatically before being sent to the terminal graphics protocol.

IRIS also renders a useful Markdown-oriented subset of embedded HTML, including common formatting, headings, links, lists, blockquotes, code, `<pre>`, `<mark>`, `<kbd>`, `<br>`, `<hr>`, and image tags. It does not execute JavaScript or behave as a web browser.

## Unicode

IRIS uses UTF-8 throughout and display-width-aware wrapping. CJK text, accented characters, Cyrillic, Greek, language symbols, combining characters, and emoji can be rendered as long as the active terminal/font provides the required glyphs.

Full bidirectional layout for right-to-left scripts depends on terminal support and is not currently treated as a dedicated layout mode.
