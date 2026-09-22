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

Markdown rendering can include richer content such as LaTeX expressions and Mermaid diagrams.

Support will be added incrementally as the project develops.

## Usage

IRIS is intended to work both with files and Unix pipes:

```bash
iris README.md
iris config.json
iris data.csv
iris document.pdf
iris database.sqlite
```

Or directly from another command:

```bash
curl https://example.com/data.json | iris
journalctl | iris
cat README.md | iris
```

IRIS automatically detects the input type and opens an appropriate interactive view.

