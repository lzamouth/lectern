//! An image of the document, loaded asynchronously from a local file or,
//! when enabled, from the web.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use crate::{document, remote};

/// Where an image comes from.
#[derive(Debug, Clone)]
pub enum Source {
	File(gio::File),
	/// An HTTP or HTTPS URL.
	Remote(String),
}

mod imp {
	use super::*;

	#[derive(Debug, Default)]
	pub struct Image {
		pub(super) picture: gtk::Picture,
		/// Size requested by the document, in pixels.
		pub(super) requested: Cell<(Option<i32>, Option<i32>)>,
		/// Size of the loaded image, in pixels.
		pub(super) natural: Cell<Option<(i32, i32)>>,
		/// Width available in the document.
		pub(super) available: Cell<i32>,
		pub(super) link: RefCell<Option<String>>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for Image {
		const NAME: &'static str = "LecternImage";
		type Type = super::Image;
		type ParentType = adw::Bin;
	}

	impl ObjectImpl for Image {}
	impl WidgetImpl for Image {}
	impl BinImpl for Image {}
}

glib::wrapper! {
	pub struct Image(ObjectSubclass<imp::Image>)
		@extends adw::Bin, gtk::Widget,
		@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Image {
	/// Creates an image showing `source`, with the alternative text `alt`.
	///
	/// `link_activated` is called with the link of the image when it is clicked.
	pub fn new(
		source: Source,
		image: &document::Image,
		alt: &str,
		link: Option<&str>,
		link_activated: impl Fn(&str) + 'static,
	) -> Self {
		let obj: Self = glib::Object::new();
		let imp = obj.imp();
		imp.requested.set((image.width, image.height));
		imp.link.replace(link.map(str::to_owned));

		imp.picture.set_can_shrink(true);
		imp.picture.set_content_fit(gtk::ContentFit::Contain);
		imp.picture.set_alternative_text(Some(alt));
		obj.set_child(Some(&imp.picture));

		let tooltip = if image.title.is_empty() {
			alt
		} else {
			&image.title
		};
		if !tooltip.is_empty() {
			obj.set_tooltip_text(Some(tooltip));
		}

		if link.is_some() {
			obj.set_cursor_from_name(Some("pointer"));
			let click = gtk::GestureClick::new();
			click.connect_released(glib::clone!(
				#[weak]
				obj,
				move |_, _, _, _| {
					if let Some(link) = obj.imp().link.borrow().as_deref() {
						link_activated(link);
					}
				}
			));
			obj.add_controller(click);
		}

		obj.update_size();
		obj.load(source, alt.to_owned());
		obj
	}

	/// Limits the width of the image to `width` pixels.
	pub fn fit(&self, width: i32) {
		self.imp().available.set(width);
		self.update_size();
	}

	fn load(&self, source: Source, alt: String) {
		glib::spawn_future_local(glib::clone!(
			#[weak(rename_to = image)]
			self,
			async move {
				let texture = match source {
					// Decode in a thread: large images would freeze the interface.
					Source::File(file) => match file.path() {
						Some(path) => {
							gio::spawn_blocking(move || gdk::Texture::from_filename(path))
								.await
								.ok()
								.and_then(Result::ok)
						}
						None => None,
					},
					Source::Remote(url) => remote::texture(&url).await,
				};
				match texture {
					Some(texture) => image.set_texture(&texture),
					None => image.show_missing(&alt),
				}
			}
		));
	}

	fn set_texture(&self, texture: &gdk::Texture) {
		let imp = self.imp();
		imp.natural.set(Some((texture.width(), texture.height())));
		imp.picture.set_paintable(Some(texture));
		self.update_size();
	}

	/// Replaces the image by an icon and its alternative text.
	fn show_missing(&self, alt: &str) {
		let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
		content.add_css_class("dimmed");
		content.append(&gtk::Image::from_icon_name("image-missing-symbolic"));
		if !alt.is_empty() {
			content.append(&gtk::Label::new(Some(alt)));
		}
		let imp = self.imp();
		imp.natural.set(None);
		imp.requested.set((None, None));
		self.set_child(Some(&content));
	}

	fn update_size(&self) {
		let imp = self.imp();
		let (width, height) =
			display_size(imp.requested.get(), imp.natural.get(), imp.available.get());
		imp.picture.set_size_request(width, height);
	}
}

/// Size at which to show an image: the requested size, or its natural size,
/// scaled down to fit in `available` pixels (when known) and keeping the
/// aspect ratio.
fn display_size(
	requested: (Option<i32>, Option<i32>),
	natural: Option<(i32, i32)>,
	available: i32,
) -> (i32, i32) {
	let (width, height) = match (requested, natural) {
		((Some(width), Some(height)), _) => (width, height),
		((Some(width), None), Some((natural_width, natural_height))) => {
			(width, scale(natural_height, width, natural_width))
		}
		((None, Some(height)), Some((natural_width, natural_height))) => {
			(scale(natural_width, height, natural_height), height)
		}
		(_, Some(natural)) => natural,
		_ => return (-1, -1),
	};
	if available > 0 && width > available {
		(available, scale(height, available, width))
	} else {
		(width, height)
	}
}

/// `value * numerator / denominator`, rounded.
fn scale(value: i32, numerator: i32, denominator: i32) -> i32 {
	if denominator == 0 {
		return value;
	}
	(f64::from(value) * f64::from(numerator) / f64::from(denominator)).round() as i32
}

#[cfg(test)]
mod tests {
	use super::display_size;

	#[test]
	fn sizes() {
		// Natural size, unknown or large enough space.
		assert_eq!(display_size((None, None), Some((200, 100)), 0), (200, 100));
		assert_eq!(
			display_size((None, None), Some((200, 100)), 500),
			(200, 100)
		);
		// Scaled down to the available width.
		assert_eq!(
			display_size((None, None), Some((1000, 500)), 400),
			(400, 200)
		);
		// Requested width or height, keeping the aspect ratio.
		assert_eq!(
			display_size((Some(100), None), Some((200, 100)), 500),
			(100, 50)
		);
		assert_eq!(
			display_size((None, Some(50)), Some((200, 100)), 500),
			(100, 50)
		);
		// Both requested: reserved before loading.
		assert_eq!(display_size((Some(80), Some(20)), None, 500), (80, 20));
		// Nothing known yet.
		assert_eq!(display_size((None, None), None, 500), (-1, -1));
	}
}
