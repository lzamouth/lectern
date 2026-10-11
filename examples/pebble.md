# Pebble

Pebble is a tiny static site generator. It turns a folder of **Markdown**
files into a fast, accessible website, with no configuration required.

> [!TIP]
> Run `pebble serve` while writing: pages are rebuilt as soon as you save.

## Features

- Instant rebuilds, even for sites with thousands of pages
- Syntax highlighting for more than 150 languages
- Responsive images, generated at build time
- [x] Feeds and sitemaps
- [ ] Multilingual sites

## Installation

Pebble is a single binary:

```sh
curl -sSf https://example.org/pebble/install.sh | sh
pebble new my-site && cd my-site
pebble serve
```

## Configuration

Settings live in `pebble.toml`, at the root of the site:

| Setting | Default | Description |
|:--------|:--------|:------------|
| `title` | — | Name of the site, shown in the page titles |
| `base_url` | `/` | Address where the site is published |
| `drafts` | `false` | Whether to publish the pages marked as drafts |
| `theme` | `pebble` | Theme used to render the pages |

```toml
title = "My notes"
base_url = "https://notes.example.org/"

[markdown]
smart_punctuation = true
```

## Writing pages

Each page starts with an optional header, followed by its content:

```markdown
---
title: Hello
date: 2026-10-11
---

Welcome to *my* new site!
```

## Contributing

Contributions are welcome! Please read the contribution guide before opening
a pull request, and run the tests with `cargo test`.

## License

Pebble is released under the MIT License.
