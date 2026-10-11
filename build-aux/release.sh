#!/bin/sh
# Prepares and publishes a release of Lectern. See RELEASING.md.
#
# Usage:
#   build-aux/release.sh prepare VERSION
#   build-aux/release.sh publish VERSION NOTES_FILE
#
# Publishing needs:
#   LECTERN_GPG_KEY      the GPG key signing the Flatpak repository
#   LECTERN_GPG_HOMEDIR  the GnuPG home of that key (optional)
#   LECTERN_PAGES_DIR    a clone of the repository hosting the Flatpak
#                        repository on GitHub Pages (default: ../lectern-flatpak)
set -eu

APP_ID=com.skunkcrafts.Lectern
REPOSITORY=lzamouth/lectern
MANIFEST=build-aux/$APP_ID.json
METAINFO=data/$APP_ID.metainfo.xml.in
RUNTIME_REPO=https://flathub.org/repo/flathub.flatpakrepo
# Branch releases are made from.
RELEASE_BRANCH=${RELEASE_BRANCH:-main}
# Address of the Flatpak repository, hosted with GitHub Pages.
PAGES_URL=${LECTERN_PAGES_URL:-https://lzamouth.github.io/lectern-flatpak}
FLATPAK_BRANCH=stable

die() {
	echo "Error: $*" >&2
	exit 1
}

check_version() {
	echo "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || die "invalid version “$1”, expected X.Y.Z"
}

check_clean_main() {
	[ "$(git branch --show-current)" = "$RELEASE_BRANCH" ] || die "not on the $RELEASE_BRANCH branch"
	[ -z "$(git status --porcelain)" ] || die "the working tree has changes"
}

prepare() {
	version=$1
	check_version "$version"
	check_clean_main
	git rev-parse -q --verify "refs/tags/v$version" >/dev/null && die "tag v$version already exists"

	date=$(date +%Y-%m-%d)
	sed -i "0,/^version = \".*\"/s//version = \"$version\"/" Cargo.toml
	sed -i "s/^\tversion: '.*',/\tversion: '$version',/" meson.build
	# Updates the version of the package in Cargo.lock.
	cargo update --offline -p lectern >/dev/null
	# The newest release comes first.
	sed -i "s|^\t<releases>|\t<releases>\n\t\t<release version=\"$version\" date=\"$date\">\n\t\t\t<url type=\"details\">https://github.com/$REPOSITORY/releases/tag/v$version</url>\n\t\t</release>|" "$METAINFO"

	meson compile -C build
	meson test -C build

	git commit -q -am "Release $version"
	git tag -a "v$version" -m "Lectern $version"
	echo "Release $version prepared: check the commit, then run"
	echo "  build-aux/release.sh publish $version NOTES_FILE"
}

publish() {
	version=$1
	notes=$2
	check_version "$version"
	check_clean_main
	[ -f "$notes" ] || die "notes file “$notes” not found"
	[ "$(git describe --exact-match --tags 2>/dev/null)" = "v$version" ] ||
		die "HEAD is not tagged v$version: run prepare first"

	gpg_key=${LECTERN_GPG_KEY:?set LECTERN_GPG_KEY to the key signing the Flatpak repository}
	gpg_homedir=${LECTERN_GPG_HOMEDIR:-}
	pages=${LECTERN_PAGES_DIR:-../lectern-flatpak}
	[ -d "$pages/.git" ] || die "$pages is not a clone of the Flatpak repository"
	[ -z "$(git -C "$pages" status --porcelain)" ] || die "$pages has changes"

	mkdir -p dist
	public_key=dist/lectern.gpg
	gpg ${gpg_homedir:+--homedir "$gpg_homedir"} --export "$gpg_key" >"$public_key"
	[ -s "$public_key" ] || die "GPG key $gpg_key not found"

	# Build into the Flatpak repository, signed, then update its summary.
	flatpak-builder --disable-rofiles-fuse --force-clean --state-dir=.flatpak-builder \
		--default-branch="$FLATPAK_BRANCH" --repo="$pages/repo" \
		--gpg-sign="$gpg_key" ${gpg_homedir:+--gpg-homedir="$gpg_homedir"} \
		build-flatpak "$MANIFEST"
	flatpak build-update-repo --generate-static-deltas --prune --prune-depth=10 \
		--title=Lectern --default-branch="$FLATPAK_BRANCH" \
		--gpg-sign="$gpg_key" ${gpg_homedir:+--gpg-homedir="$gpg_homedir"} \
		"$pages/repo"
	write_install_files "$pages" "$public_key"

	# The bundle installs the repository too, for updates.
	bundle=dist/lectern-$version.flatpak
	flatpak build-bundle "$pages/repo" "$bundle" "$APP_ID" "$FLATPAK_BRANCH" \
		--repo-url="$PAGES_URL/repo/" --runtime-repo="$RUNTIME_REPO" --gpg-keys="$public_key"

	echo
	echo "Ready to publish Lectern $version:"
	echo "  - push $RELEASE_BRANCH and tag v$version to https://github.com/$REPOSITORY"
	echo "  - create the release with $notes and $bundle"
	echo "  - push the Flatpak repository in $pages to $PAGES_URL"
	printf "Publish? [y/N] "
	read -r answer
	[ "$answer" = y ] || [ "$answer" = Y ] || die "cancelled"

	git push origin "$RELEASE_BRANCH" "v$version"
	gh release create "v$version" "$bundle" --repo "$REPOSITORY" \
		--title "Lectern $version" --notes-file "$notes" --verify-tag
	git -C "$pages" add -A
	git -C "$pages" commit -q -m "Lectern $version"
	git -C "$pages" push -q origin HEAD
}

# Writes the files installing Lectern and its repository, and the page
# linking to them.
write_install_files() {
	pages=$1
	key=$(base64 -w0 "$2")
	cat >"$pages/lectern.flatpakref" <<-EOF
		[Flatpak Ref]
		Title=Lectern
		Name=$APP_ID
		Branch=$FLATPAK_BRANCH
		Url=$PAGES_URL/repo/
		SuggestRemoteName=lectern
		Homepage=https://github.com/$REPOSITORY
		Icon=$PAGES_URL/$APP_ID.svg
		RuntimeRepo=$RUNTIME_REPO
		IsRuntime=false
		GPGKey=$key
	EOF
	cat >"$pages/lectern.flatpakrepo" <<-EOF
		[Flatpak Repo]
		Title=Lectern
		Url=$PAGES_URL/repo/
		Homepage=https://github.com/$REPOSITORY
		Comment=Native Markdown reader for GNOME
		Icon=$PAGES_URL/$APP_ID.svg
		GPGKey=$key
	EOF
	cp "data/icons/hicolor/scalable/apps/$APP_ID.svg" "$pages/"
	# GitHub Pages must serve the files as they are.
	touch "$pages/.nojekyll"
	cat >"$pages/index.html" <<-EOF
		<!doctype html>
		<html lang="en">
		<meta charset="utf-8">
		<title>Lectern</title>
		<h1>Lectern</h1>
		<p>Flatpak repository of <a href="https://github.com/$REPOSITORY">Lectern</a>, a native Markdown reader for GNOME.</p>
		<p><a href="lectern.flatpakref">Install Lectern</a>, or run:</p>
		<pre>flatpak install --user $PAGES_URL/lectern.flatpakref</pre>
		<p>Updates are then installed like for any other Flatpak application.</p>
		</html>
	EOF
}

[ $# -ge 1 ] || die "usage: $0 prepare VERSION | publish VERSION NOTES_FILE"
cd "$(dirname "$0")/.."
case $1 in
	prepare)
		[ $# -eq 2 ] || die "usage: $0 prepare VERSION"
		prepare "$2"
		;;
	publish)
		[ $# -eq 3 ] || die "usage: $0 publish VERSION NOTES_FILE"
		publish "$2" "$3"
		;;
	*)
		die "unknown command “$1”"
		;;
esac
