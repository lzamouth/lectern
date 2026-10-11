#!/bin/sh
# Prepares and publishes a release of Lectern. See RELEASING.md.
#
# Usage:
#   build-aux/release.sh prepare VERSION
#   build-aux/release.sh publish VERSION NOTES_FILE
set -eu

APP_ID=com.skunkcrafts.Lectern
REPOSITORY=lzamouth/lectern
MANIFEST=build-aux/$APP_ID.json
METAINFO=data/$APP_ID.metainfo.xml.in
RUNTIME_REPO=https://flathub.org/repo/flathub.flatpakrepo

die() {
	echo "Error: $*" >&2
	exit 1
}

check_version() {
	echo "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || die "invalid version “$1”, expected X.Y.Z"
}

check_clean_main() {
	[ "$(git branch --show-current)" = main ] || die "not on the main branch"
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

	bundle=dist/lectern-$version.flatpak
	mkdir -p dist
	flatpak-builder --disable-rofiles-fuse --force-clean --state-dir=.flatpak-builder \
		--repo=build-flatpak-repo build-flatpak "$MANIFEST"
	flatpak build-bundle build-flatpak-repo "$bundle" "$APP_ID" --runtime-repo="$RUNTIME_REPO"

	echo
	echo "Ready to publish Lectern $version on https://github.com/$REPOSITORY:"
	echo "  - push main and tag v$version"
	echo "  - create the release with $notes and $bundle"
	printf "Publish? [y/N] "
	read -r answer
	[ "$answer" = y ] || [ "$answer" = Y ] || die "cancelled"

	git push origin main "v$version"
	gh release create "v$version" "$bundle" --repo "$REPOSITORY" \
		--title "Lectern $version" --notes-file "$notes" --verify-tag
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
