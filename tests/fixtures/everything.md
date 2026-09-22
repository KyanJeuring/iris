# IRIS Markdown Fixture

A paragraph with **bold**, *italic*, ~~strikethrough~~, `inline code`, and an [IRIS link](https://github.com/KyanJeuring/iris).

## Lists

- First item
  - Nested item
  - [x] Finished task
  - [ ] Open task
- Second item

1. Ordered
2. Another

## Quotes

> A normal quote that should wrap when the terminal is narrow.

> [!IMPORTANT]
> This is an important GitHub-style alert.

## Table

| Format | Status | Notes |
| :--- | :---: | ---: |
| Markdown | Ready | Rendered |
| JSON | Later | Planned |

## Code

```rust
fn main() {
    println!("Hello from IRIS");
}
```

## Image

![IRIS logo](assets/iris.png)

## HTML

<p><strong>HTML strong text</strong> with <mark>highlighting</mark>, <kbd>Ctrl</kbd> and a <a href="https://example.com">link</a>.</p>

<p align="center"><img src="assets/iris.png" alt="Centered IRIS image"></p>

---

## Math

Inline math: $E = mc^2$.

$$x^2 + y^2 = z^2$$

### More HTML

<div><u>underlined</u>, <del>deleted</del>, <code>inline HTML code</code><br>next line</div>

<table><tr><th>Name</th><th>Value</th></tr><tr><td>IRIS</td><td>1.0</td></tr></table>

<a href="https://github.com/KyanJeuring/iris"><img src="assets/iris.png" alt="Linked IRIS image"></a>
