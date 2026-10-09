//! Development helpers, only compiled in debug builds.
//!
//! `LECTERN_SNAPSHOT=/path/to/file.png` saves a picture of the first window
//! once a document is loaded, then quits the application.
//! `LECTERN_SNAPSHOT_SIZE=WIDTHxHEIGHT` resizes the window before the picture.
//! `LECTERN_SNAPSHOT_SEARCH=text` searches the text before the picture.

use std::time::Duration;

use gtk::prelude::*;
use gtk::{glib, graphene};

pub fn snapshot_if_requested(window: &impl IsA<gtk::Window>) {
	let Some(path) = std::env::var_os("LECTERN_SNAPSHOT") else {
		return;
	};
	let window = window.as_ref().clone();
	let size = std::env::var("LECTERN_SNAPSHOT_SIZE")
		.ok()
		.and_then(|size| {
			let (width, height) = size.split_once('x')?;
			Some((width.parse().ok()?, height.parse().ok()?))
		});
	if let Some((width, height)) = size {
		glib::timeout_add_local_once(
			Duration::from_millis(300),
			glib::clone!(
				#[weak]
				window,
				move || window.set_default_size(width, height)
			),
		);
	}
	glib::timeout_add_local_once(Duration::from_millis(800), move || {
		let (width, height) = (window.width(), window.height());
		let paintable = gtk::WidgetPaintable::new(Some(&window));
		let snapshot = gtk::Snapshot::new();
		paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
		let bounds = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
		match (snapshot.to_node(), window.renderer()) {
			(Some(node), Some(renderer)) => {
				let texture = renderer.render_texture(&node, Some(&bounds));
				if let Err(err) = texture.save_to_png(&path) {
					eprintln!("Unable to save the snapshot: {err}");
				}
			}
			_ => eprintln!("Unable to render the snapshot"),
		}
		if let Some(app) = window.application() {
			app.quit();
		}
	});
}
