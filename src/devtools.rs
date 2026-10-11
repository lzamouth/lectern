//! Development helpers, only compiled in debug builds.
//!
//! `LECTERN_SNAPSHOT=/path/to/file.png` saves a picture of the first window
//! once a document is loaded, then quits the application.
//! `LECTERN_SNAPSHOT_SIZE=WIDTHxHEIGHT` resizes the window before the picture.
//! `LECTERN_SNAPSHOT_SEARCH=text` searches the text before the picture.
//! `LECTERN_SNAPSHOT_DELAY=milliseconds` waits longer before the picture.
//! `LECTERN_SNAPSHOT_ACTION=app.name` activates an action before the picture.
//! `LECTERN_SNAPSHOT_STEPS=link:url;action:win.name;…` follows links and
//! activates actions, one every half second, before the picture.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gtk::prelude::*;
use gtk::{glib, graphene};

/// Delay between the steps of `LECTERN_SNAPSHOT_STEPS`.
const STEP_DELAY: Duration = Duration::from_millis(500);

static STARTED: AtomicBool = AtomicBool::new(false);

/// Takes the picture requested by the environment, once. `follow_link` is
/// used for the `link:` steps.
pub fn snapshot_if_requested(window: &impl IsA<gtk::Window>, follow_link: impl Fn(&str) + 'static) {
	let Some(path) = std::env::var_os("LECTERN_SNAPSHOT") else {
		return;
	};
	if STARTED.swap(true, Ordering::Relaxed) {
		return;
	}
	let window = window.as_ref().clone();
	if let Ok(action) = std::env::var("LECTERN_SNAPSHOT_ACTION") {
		let _ = window.activate_action(&action, None);
	}

	let follow_link = Rc::new(follow_link);
	let steps = std::env::var("LECTERN_SNAPSHOT_STEPS").unwrap_or_default();
	let steps: Vec<String> = steps
		.split(';')
		.filter(|step| !step.is_empty())
		.map(str::to_owned)
		.collect();
	let steps_duration = STEP_DELAY * steps.len() as u32;
	for (index, step) in steps.into_iter().enumerate() {
		let window = window.clone();
		let follow_link = follow_link.clone();
		glib::timeout_add_local_once(STEP_DELAY * (index as u32 + 1), move || {
			if let Some(url) = step.strip_prefix("link:") {
				follow_link(url);
			} else if let Some(action) = step.strip_prefix("action:") {
				let _ = window.activate_action(action, None);
			}
		});
	}

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

	let delay = std::env::var("LECTERN_SNAPSHOT_DELAY")
		.ok()
		.and_then(|delay| delay.parse().ok())
		.map_or(Duration::from_millis(800), Duration::from_millis);
	glib::timeout_add_local_once(steps_duration + delay, move || {
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
