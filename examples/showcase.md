---
title: Lectern showcase
---

# Lectern showcase

This document exercises the Markdown features supported by **Lectern**.
It grows with each development step.

## Inline styles

Text can be *emphasized*, **strong**, ***both***, ~~struck through~~ or
contain `inline code`. A line can end with a hard break  
like this one, or with a backslash\
like that one.

## Links

- External: [GNOME](https://www.gnome.org), and an autolink: https://github.com/lzamouth/lectern
- Anchor: jump to [the last section](#the-last-section)
- Relative Markdown file: [README](../README.md)
- Relative other file: [the license](../LICENSE)

### Third level heading

#### Fourth level heading

##### Fifth level heading

###### Sixth level heading

## Lists

- A bullet list item
- An item with **nested** content:
  1. First ordered item
  2. Second ordered item
     - Deeper bullet
       - Even deeper bullet
  3. Third item, which is long enough to wrap on several lines when the
     window is narrow, so that the hanging indentation can be checked.
- Last bullet item

8. An ordered list
9. starting at eight,
10. with two-digit numbers.

Task list:

- [x] Write the skeleton
- [x] Render basic Markdown
- [ ] Render tables

A loose list:

* First paragraph item.

* Second paragraph item, with a second paragraph:

  Continuation paragraph of the second item.

## Quotes

> A block quote can contain *styled text*
> on several lines.
>
> > And nested quotes,
> > with a deeper bar.
>
> - Or even lists.

## Raw HTML

<div align="center">
  <img src="logo.png" alt="Logo">
  <p>Text inside an <b>HTML</b> block is kept, tags are hidden.</p>
</div>

Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy.<!-- a hidden comment -->

## Tables

Drag the vertical lines to resize a column, double-click them to fit the
column to its content.

| Feature | Status | Notes |
|:--------|:------:|------:|
| Headings and paragraphs | ✓ | Step 1 |
| Lists and quotes | ✓ | Step 2 |
| Code blocks | ✓ | Step 3 |
| Tables with *resizable* columns | ✓ | Step 4, with a [link](#lectern-showcase) |
| Images | — | Step 5 |

A wide table scrolls horizontally when its columns cannot shrink further:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `margin` | integer | `24` | Space between the window edges and the text, in pixels. It is applied on both sides of the content. |
| `font` | string | system | Font used for proportional text. When unset, the document font of the desktop is used. |
| `monospace-font` | string | system | Font used for code. When unset, the monospace font of the desktop is used instead. |

## Images

A wide image is scaled down to the width of the window:

![A stylized landscape](images/landscape.png "Landscape")

Small images flow with the text, like this icon
![Lectern icon](../data/icons/hicolor/scalable/apps/com.skunkcrafts.Lectern.svg)
inside a sentence. HTML images can set their size, and can be links:

<p align="center">
  <a href="https://github.com/lzamouth/lectern"><img src="../data/icons/hicolor/scalable/apps/com.skunkcrafts.Lectern.svg" width="64" alt="Lectern"></a>
  <img src="images/landscape.png" height="64" alt="Landscape">
</p>

A missing image shows its alternative text: ![Missing picture](images/missing.png)

Remote images are not loaded: ![Remote badge](https://example.org/badge.svg)

## Code

Fenced code blocks are highlighted according to their language:

```rust
fn main() {
	let name = "Lectern";
	// Long lines scroll horizontally instead of wrapping, which keeps the code readable even when the window is narrow.
	println!("Hello, {name}!");
}
```

```python
def greet(name: str) -> str:
    return f"Hello, {name}!"
```

```sh
meson setup build && meson compile -C build
```

A block without language, inside a list:

1. Run:

   ```
   plain text block
   ```

---

## The last section

Paragraphs are separated by vertical space. Lorem ipsum dolor sit amet,
consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et
dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco
laboris nisi ut aliquip ex ea commodo consequat.
