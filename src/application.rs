use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config;
use crate::window::LecternWindow;

mod imp {
	use super::*;

	#[derive(Debug, Default)]
	pub struct LecternApplication;

	#[glib::object_subclass]
	impl ObjectSubclass for LecternApplication {
		const NAME: &'static str = "LecternApplication";
		type Type = super::LecternApplication;
		type ParentType = adw::Application;
	}

	impl ObjectImpl for LecternApplication {}

	impl ApplicationImpl for LecternApplication {
		fn startup(&self) {
			self.parent_startup();
			sourceview5::init();
			let app = self.obj();
			app.setup_actions();
			app.setup_accels();
		}

		fn activate(&self) {
			let app = self.obj();
			match app.active_window() {
				Some(window) => window.present(),
				None => LecternWindow::new(&app).present(),
			}
		}

		fn open(&self, files: &[gio::File], _hint: &str) {
			let app = self.obj();
			for file in files {
				let window = app.window_for_open();
				window.open_file(file);
				window.present();
			}
		}
	}

	impl GtkApplicationImpl for LecternApplication {}
	impl AdwApplicationImpl for LecternApplication {}
}

glib::wrapper! {
	pub struct LecternApplication(ObjectSubclass<imp::LecternApplication>)
		@extends adw::Application, gtk::Application, gio::Application,
		@implements gio::ActionGroup, gio::ActionMap;
}

impl LecternApplication {
	pub fn new() -> Self {
		glib::Object::builder()
			.property("application-id", config::APP_ID)
			.property("flags", gio::ApplicationFlags::HANDLES_OPEN)
			.property("resource-base-path", "/com/skunkcrafts/Lectern")
			.build()
	}

	/// Returns the active window if it shows no document yet, or a new window.
	fn window_for_open(&self) -> LecternWindow {
		self.active_window()
			.and_downcast::<LecternWindow>()
			.filter(|window| window.file().is_none())
			.unwrap_or_else(|| LecternWindow::new(self))
	}

	fn setup_actions(&self) {
		let new_window = gio::ActionEntry::builder("new-window")
			.activate(|app: &Self, _, _| LecternWindow::new(app).present())
			.build();
		let about = gio::ActionEntry::builder("about")
			.activate(|app: &Self, _, _| app.show_about())
			.build();
		let quit = gio::ActionEntry::builder("quit")
			.activate(|app: &Self, _, _| app.quit())
			.build();
		self.add_action_entries([new_window, about, quit]);
	}

	fn setup_accels(&self) {
		self.set_accels_for_action("app.new-window", &["<Control>n"]);
		self.set_accels_for_action("app.quit", &["<Control>q"]);
		self.set_accels_for_action("win.open", &["<Control>o"]);
		self.set_accels_for_action("win.find", &["<Control>f"]);
		self.set_accels_for_action("win.find-next", &["<Control>g"]);
		self.set_accels_for_action("win.find-previous", &["<Control><Shift>g"]);
		self.set_accels_for_action("window.close", &["<Control>w"]);
	}

	fn show_about(&self) {
		let about = adw::AboutDialog::builder()
			.application_name("Lectern")
			.application_icon(config::APP_ID)
			.developer_name("Skunkcrafts")
			.version(config::VERSION)
			.website("https://github.com/lzamouth/lectern")
			.issue_url("https://github.com/lzamouth/lectern/issues")
			.license_type(gtk::License::MitX11)
			.copyright("© 2026 Skunkcrafts")
			// Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
			.translator_credits(gettext("translator-credits"))
			.build();
		about.present(self.active_window().as_ref());
	}
}

impl Default for LecternApplication {
	fn default() -> Self {
		Self::new()
	}
}
