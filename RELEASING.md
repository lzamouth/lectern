# Releasing Lectern

Releases are published on GitHub, with a Flatpak bundle, and in a Flatpak
repository hosted with GitHub Pages, at
<https://lzamouth.github.io/lectern-flatpak/>. Installations from the
repository, or from a bundle, get updates from that repository.

## Requirements

- `flatpak-builder`, the GNOME 51 SDK and the Rust extension:

  ```sh
  flatpak install flathub org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08
  ```

- The GitHub CLI (`gh`), logged in with push access to the repositories
- A configured `build` directory (`meson setup build`)
- The GPG key signing the Flatpak repository
- A clone of the repository hosting the Flatpak repository

## One-time setup

1. Create a GPG key dedicated to signing the Flatpak repository, and keep its
   private part safe:

   ```sh
   gpg --quick-generate-key "Lectern Flatpak <lionel@skunkcrafts.com>" ed25519 sign never
   ```

2. Create the repository hosting the Flatpak repository, serve it with GitHub
   Pages from its `main` branch, and clone it next to this one:

   ```sh
   gh repo create lzamouth/lectern-flatpak --public --description "Flatpak repository of Lectern"
   git clone git@github.com:lzamouth/lectern-flatpak.git ../lectern-flatpak
   git -C ../lectern-flatpak commit --allow-empty -m "Create the Flatpak repository"
   git -C ../lectern-flatpak push -u origin main
   gh api repos/lzamouth/lectern-flatpak/pages -f "source[branch]=main" -f "source[path]=/"
   ```

The publish step needs the key in `LECTERN_GPG_KEY` (its fingerprint), and
optionally `LECTERN_GPG_HOMEDIR` if it is not in the default GnuPG home. The
clone is expected in `../lectern-flatpak`, or in `LECTERN_PAGES_DIR`.

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
   LECTERN_GPG_KEY=FINGERPRINT build-aux/release.sh publish 0.2.0 path/to/notes.md
   ```

   This builds the release into the signed Flatpak repository, updates the
   `.flatpakref` and `.flatpakrepo` files installing it, and builds the bundle
   in `dist/`. It then asks for confirmation before pushing `main` and the tag,
   creating the GitHub release with the notes and the bundle, and pushing the
   Flatpak repository to GitHub Pages. GnuPG asks for the passphrase of the
   key while signing.

## Versions

Versions follow `MAJOR.MINOR.PATCH`. Bug fix releases increase `PATCH`,
releases with new features increase `MINOR`.
