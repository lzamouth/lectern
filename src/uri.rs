//! Resolution of the URLs found in documents.

use gtk::prelude::*;
use gtk::{gio, glib};

/// The local file designated by `url`: a `file:` URI, or a path, absolute or
/// relative to `base_dir`. A `#fragment` is ignored.
///
/// Returns `None` for other URI schemes, or for relative paths without a base.
pub fn local_file(url: &str, base_dir: Option<&gio::File>) -> Option<gio::File> {
	match glib::Uri::peek_scheme(url) {
		Some(scheme) if scheme == "file" => Some(gio::File::for_uri(url)),
		Some(_) => None,
		None => {
			let path = url.split_once('#').map_or(url, |(path, _)| path);
			let path = glib::Uri::unescape_string(path, None::<&str>)
				.map_or_else(|| path.to_owned(), |path| path.to_string());
			if path.starts_with('/') {
				Some(gio::File::for_path(path))
			} else {
				base_dir.map(|dir| dir.resolve_relative_path(path))
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn resolves_local_files() {
		let base = gio::File::for_path("/docs");
		let path = |url| local_file(url, Some(&base)).and_then(|file| file.path());
		assert_eq!(
			path("img/a%20b.png").unwrap().to_str(),
			Some("/docs/img/a b.png")
		);
		assert_eq!(path("../x.md#title").unwrap().to_str(), Some("/x.md"));
		assert_eq!(path("/abs.png").unwrap().to_str(), Some("/abs.png"));
		assert_eq!(path("file:///f.png").unwrap().to_str(), Some("/f.png"));
		assert!(path("https://example.org/a.png").is_none());
		assert!(local_file("a.png", None).is_none());
	}
}
