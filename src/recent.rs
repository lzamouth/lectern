//! Documents recently opened with the application, from the desktop history.

use gtk::prelude::*;
use gtk::{gio, glib};

/// Maximum number of recent documents shown.
const MAX_RECENT: usize = 10;

/// Adds `file` to the recently used documents.
pub fn add(file: &gio::File) {
	gtk::RecentManager::default().add_item(&file.uri());
}

/// Existing documents recently opened with this application, most recent first.
pub fn files() -> Vec<gio::File> {
	let application_name = glib::application_name().unwrap_or_default();
	let mut items: Vec<gtk::RecentInfo> = gtk::RecentManager::default()
		.items()
		.into_iter()
		.filter(|info| info.has_application(&application_name) && info.exists())
		.collect();
	items.sort_by_key(|info| std::cmp::Reverse(info.modified().to_unix()));
	items
		.iter()
		.take(MAX_RECENT)
		.map(|info| gio::File::for_uri(&info.uri()))
		.collect()
}
