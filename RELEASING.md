# Releasing Lectern

Releases are published on GitHub, with a Flatpak bundle installable with
`flatpak install --user lectern-X.Y.Z.flatpak`.

## Requirements

- `flatpak-builder`, the GNOME 51 SDK and the Rust extension:

  ```sh
  flatpak install flathub org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08
  ```

- The GitHub CLI (`gh`), logged in with push access to the repository
- A configured `build` directory (`meson setup build`)

## Steps

1. Make sure `main` is up to date, the working tree is clean and the tests
   pass.

2. Prepare the release, here 0.2.0:

   ```sh
   build-aux/release.sh prepare 0.2.0
   ```

   This sets the version in `Cargo.toml`, `Cargo.lock` and `meson.build`, adds
   the release to the AppStream metainfo with a link to its GitHub page, builds,
   runs the tests, commits “Release 0.2.0” and tags `v0.2.0`. Nothing is
   pushed: review the commit, and if needed undo it with
   `git tag -d v0.2.0 && git reset --hard HEAD~1`.

3. Write the release notes in a Markdown file, outside of the repository or in
   `dist/` (ignored by git).

4. Publish the release:

   ```sh
   build-aux/release.sh publish 0.2.0 path/to/notes.md
   ```

   This builds the Flatpak bundle in `dist/`, then asks for confirmation before
   pushing `main` and the tag and creating the GitHub release with the notes
   and the bundle.

## Versions

Versions follow `MAJOR.MINOR.PATCH`. Bug fix releases increase `PATCH`,
releases with new features increase `MINOR`.
