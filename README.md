# IRIS (Interactive Renderer for Information Sources)

### Terminal-native viewing for documents, data, and everything in between

IRIS is a general-purpose terminal viewer for rendering and exploring files from a single interface.

Instead of switching between separate tools for text, Markdown, images, PDFs, structured data, and other formats, IRIS aims to provide one consistent terminal-native workflow for inspecting information.

Think of it as a more visual and extensible `less` for modern files and data.

## Supported content

IRIS currently supports:

- Plain text and logs
- Standard input
- Markdown
- Images: PNG, JPEG, GIF, WebP, BMP, ICO, and SVG
- PDF documents

IRIS detects supported file types automatically from their extension and, where possible, their contents.

The project is designed to grow beyond these formats into structured data such as JSON, YAML, TOML, CSV, and SQLite while keeping the same terminal-native interface.

## Features

### Text and standard input

Plain text can be opened directly or piped into IRIS. Long lines can be wrapped to the terminal width or viewed using horizontal scrolling.

IRIS supports mouse text selection, copying selected text, searching, mouse-wheel and trackpad scrolling, and Vim-style keyboard navigation.

### Markdown

Markdown is rendered as a structured terminal document rather than displayed as raw source. Supported elements include:

- Headings
- Ordered and unordered lists
- Task lists
- Tables
- Blockquotes
- GitHub-style alerts, including custom alert titles
- Fenced code blocks with syntax highlighting
- Inline code
- Emphasis, strong text, and strikethrough
- Links
- Horizontal rules
- Markdown images
- Embedded HTML for common document elements

Markdown headings can be navigated directly, and links or images can be opened with the mouse.

### Images

IRIS can open raster images and SVG files directly. Images are fitted to the available terminal area and can be zoomed and panned interactively.

SVG files are rasterized automatically before display. IRIS detects terminal graphics capabilities and can use Kitty, Sixel, or iTerm2 image protocols, with a terminal-cell fallback when native graphics are unavailable.

The same graphics support is used for images embedded in Markdown documents.

### PDFs

PDF documents are rendered directly inside the terminal with support for mixed page sizes and orientations.

The PDF viewer provides:

- Automatic, one-page, and two-page layouts
- Landscape-aware page placement
- Zooming and panning
- Page-row navigation
- Mouse-wheel and trackpad scrolling
- Search across extractable PDF text
- Match highlighting directly on rendered pages
- Next and previous result navigation

PDF search uses the document's text layer, including invisible OCR text when the PDF already contains one. Pure image-only scans without a text layer are not searchable unless OCR has been applied to the document beforehand.

## Installation

### Cargo

Install IRIS from crates.io:

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

## Usage

Open a file directly:

```bash
iris README.md
iris notes.txt
iris image.png
iris document.pdf
```

Read from standard input:

```bash
journalctl | iris
cat README.md | iris --format markdown
```

IRIS detects supported file types automatically. Standard input is treated as plain text by default; use `--format markdown` when piped input should be rendered as Markdown.

Useful command-line options include:

```bash
iris --help
iris --version
iris --format markdown
iris --theme ember README.md
iris --icons unicode README.md
iris --wrap README.md
iris --no-wrap README.md
iris --list-themes
```

## Navigation

IRIS uses a common set of navigation controls across its viewers, with additional controls enabled when they apply to the active format.

### General

| Key | Action |
| --- | --- |
| `j` / `↓` | Scroll down |
| `k` / `↑` | Scroll up |
| `←` / `→` | Scroll horizontally |
| `Ctrl+D` / `Ctrl+U` | Half page down / up |
| `PgDn` / `PgUp` | Page down / up |
| `g` / `G` | Top / bottom |
| Mouse wheel | Scroll vertically |
| `Shift` + mouse wheel | Scroll horizontally |
| Trackpad | Scroll vertically or horizontally |
| `?` | Open help |
| `q` | Quit IRIS |

The help window is scrollable when its contents do not fit in the available terminal space.

Format-specific controls are shown in the status bar and can always be explored with `?`.

### Search

Search is available for plain text, Markdown, and PDFs with an extractable text layer.

| Key | Action |
| --- | --- |
| `/` | Start search |
| `Enter` | Accept search |
| `n` / `N` | Next / previous match |
| `Esc` | Clear search and highlights |

Search uses smart-case matching: lowercase queries are case-insensitive, while a query containing uppercase characters is case-sensitive.

## Configuration

IRIS stores its user configuration at:

```text
~/.config/iris/iris.conf
```

The file is created automatically when IRIS is started and no configuration exists. It uses TOML syntax despite the `.conf` extension.

Example configuration:

```toml
theme = "ember"
icons = "nerd-font"
wrap = true
tab_width = 4
```

Available settings:

| Setting | Description | Default |
| --- | --- | --- |
| `theme` | Theme name or custom theme | `"ember"` |
| `icons` | `"nerd-font"` or `"unicode"` | `"nerd-font"` |
| `wrap` | Wrap long prose lines | `true` |
| `tab_width` | Number of spaces used for tabs, from 1 to 16 | `4` |

Command-line options override the corresponding configuration values for the current invocation.

## Themes

Ember is the built-in default theme.

Custom themes can be placed in:

```text
~/.config/iris/themes/
```

Use `iris --list-themes` to list the themes IRIS can find and `--theme <name>` to select one.

IRIS uses Nerd Font icons by default. If the active terminal font does not provide Nerd Font glyphs, use the portable Unicode icon set:

```bash
iris --icons unicode README.md
```

or set:

```toml
icons = "unicode"
```

in `iris.conf`.

## Unicode

IRIS uses UTF-8 throughout and display-width-aware wrapping. CJK text, accented characters, Cyrillic, Greek, language symbols, combining characters, and emoji can be rendered as long as the active terminal and font provide the required glyphs.

Full bidirectional layout for right-to-left scripts depends on terminal support and is not currently treated as a dedicated layout mode.

## License

IRIS is licensed under the MIT License. See [LICENSE](LICENSE) for details.
