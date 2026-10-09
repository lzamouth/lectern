# Lectern

Lectern is a native Markdown reader for GNOME, written in Rust with GTK 4 and
libadwaita. Documents are rendered with GTK widgets: no web engine, no HTML.

> **Status:** early development.

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
