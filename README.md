# Lectern

Lectern is a native Markdown reader for GNOME, written in Rust with GTK 4 and
libadwaita. Documents are rendered with GTK widgets: no web engine, no HTML.

> **Status:** early development.

## Features

- CommonMark and GitHub Flavored Markdown: headings, emphasis, lists and task
  lists, block quotes, alerts, code blocks with syntax highlighting, tables,
  links, images, footnotes and horizontal rules
- Table columns resizable with the mouse
- Search in the whole document, including code blocks and tables, with case
  and whole word options
- Automatic reload when the file changes on disk
- Recent files, drag and drop, and opening from the file manager
- Light and dark styles, configurable margin, fonts and zoom
- Text selection and copy, including code blocks and tables
- Available in English and French

## Usage

```sh
lectern README.md
cat README.md | lectern -
```

| Shortcut | Action |
|----------|--------|
| <kbd>Ctrl</kbd>+<kbd>O</kbd> | Open a file |
| <kbd>Ctrl</kbd>+<kbd>N</kbd> | New window |
| <kbd>Alt</kbd>+<kbd>←</kbd>, <kbd>Alt</kbd>+<kbd>→</kbd> | Back and forward, after following links |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> | Search |
| <kbd>Ctrl</kbd>+<kbd>G</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>G</kbd> | Next and previous match |
| <kbd>Ctrl</kbd>+<kbd>+</kbd>, <kbd>Ctrl</kbd>+<kbd>-</kbd>, <kbd>Ctrl</kbd>+<kbd>0</kbd> | Zoom in, zoom out, reset zoom |
| <kbd>Ctrl</kbd>+<kbd>,</kbd> | Preferences |
| <kbd>Ctrl</kbd>+<kbd>W</kbd> | Close the window |
| <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Quit |

## Building

### Dependencies

- Rust (stable) and Cargo
- Meson ≥ 1.4 and Ninja
- GTK ≥ 4.18, libadwaita ≥ 1.7, GtkSourceView ≥ 5.10
- blueprint-compiler, gettext, desktop-file-utils, appstream

On Debian/Ubuntu:

```sh
sudo apt install cargo meson libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev \
	blueprint-compiler gettext desktop-file-utils appstream
```

### Build and run from the source tree

```sh
meson setup build
meson compile -C build
meson devenv -C build -w . lectern path/to/file.md
```

`meson devenv` points the application to the GSettings schema and translations
compiled in the build directory. `-w .` keeps the current directory, so that
relative paths work.

### Install

```sh
meson setup build --prefix=/usr/local
meson compile -C build
sudo meson install -C build
```

### Flatpak

The manifest is in `build-aux/`. It needs `flatpak-builder`, the GNOME 51 SDK
and the Rust extension:

```sh
flatpak install flathub org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08
flatpak-builder --user --install --force-clean build-flatpak build-aux/com.skunkcrafts.Lectern.json
flatpak run com.skunkcrafts.Lectern
```

The application reads the file system without being able to write to it,
so that relative images and links work and changed files are reloaded.

### Tests

```sh
meson test -C build
```

## Translations

Translations use gettext and live in `po/`. To update them after changing
translatable strings:

```sh
meson compile -C build lectern-pot
meson compile -C build lectern-update-po
```

## License

Lectern is released under the [MIT License](LICENSE).
