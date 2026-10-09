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
