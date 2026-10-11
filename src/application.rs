use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::appearance;
use crate::config;
use crate::preferences_dialog::LecternPreferencesDialog;
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
			appearance::init();
			let app = self.obj();
			app.setup_actions();
			app.setup_accels();
		}

		fn activate(&self) {
			let app = self.obj();
			match app.active_window() {
				Some(window) => window.present(),
				None => {
					let window = LecternWindow::new(&app);
					window.present();
					#[cfg(debug_assertions)]
					crate::devtools::snapshot_if_requested(&window, |_| {});
				}
			}
		}

		fn handle_local_options(
			&self,
			options: &glib::VariantDict,
		) -> std::ops::ControlFlow<glib::ExitCode> {
			if options.contains("version") {
				println!("Lectern {}", config::VERSION);
				return std::ops::ControlFlow::Break(glib::ExitCode::SUCCESS);
			}
			self.parent_handle_local_options(options)
		}

		fn command_line(&self, command_line: &gio::ApplicationCommandLine) -> glib::ExitCode {
			let app = self.obj();
			let arguments = command_line.arguments();
			let paths = arguments.get(1..).unwrap_or_default();
			if paths.is_empty() {
				app.activate();
				return glib::ExitCode::SUCCESS;
			}
			for path in paths {
				let window = app.window_for_open();
				if path == "-" {
					if let Some(stdin) = command_line.stdin() {
						let base_dir = command_line.cwd().map(gio::File::for_path);
						window.open_stream(stdin, base_dir);
					}
				} else {
					window.open_file(&command_line.create_file_for_arg(path));
				}
				window.present();
			}
			glib::ExitCode::SUCCESS
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
		let app: Self = glib::Object::builder()
			.property("application-id", config::APP_ID)
			.property(
				"flags",
				gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::HANDLES_COMMAND_LINE,
			)
			.property("resource-base-path", "/com/skunkcrafts/Lectern")
			.build();
		// Snapshots must not be taken by an already running instance.
		#[cfg(debug_assertions)]
		if std::env::var_os("LECTERN_SNAPSHOT").is_some() {
			app.set_flags(app.flags() | gio::ApplicationFlags::NON_UNIQUE);
		}
		app.add_main_option(
			"version",
			glib::Char::from(b'v'),
			glib::OptionFlags::NONE,
			glib::OptionArg::None,
			&gettext("Show the version"),
			None,
		);
		app.set_option_context_parameter_string(Some(&gettext("[FILE…]")));
		app.set_option_context_summary(Some(&gettext(
			"Read Markdown documents. Use “-” to read the standard input.",
		)));
		app
	}

	/// Returns the active window if it shows no document yet, or a new window.
	fn window_for_open(&self) -> LecternWindow {
		self.active_window()
			.and_downcast::<LecternWindow>()
			.filter(|window| !window.has_document())
			.unwrap_or_else(|| LecternWindow::new(self))
	}

	fn setup_actions(&self) {
		let new_window = gio::ActionEntry::builder("new-window")
			.activate(|app: &Self, _, _| LecternWindow::new(app).present())
			.build();
		let preferences = gio::ActionEntry::builder("preferences")
			.activate(|app: &Self, _, _| {
				LecternPreferencesDialog::new().present(app.active_window().as_ref());
			})
			.build();
		let about = gio::ActionEntry::builder("about")
			.activate(|app: &Self, _, _| app.show_about())
			.build();
		let quit = gio::ActionEntry::builder("quit")
			.activate(|app: &Self, _, _| app.quit())
			.build();
		self.add_action_entries([new_window, preferences, about, quit]);
	}

	fn setup_accels(&self) {
		self.set_accels_for_action("app.new-window", &["<Control>n"]);
		self.set_accels_for_action("app.quit", &["<Control>q"]);
		self.set_accels_for_action("app.preferences", &["<Control>comma"]);
		self.set_accels_for_action("win.open", &["<Control>o"]);
		self.set_accels_for_action("win.find", &["<Control>f"]);
		self.set_accels_for_action("win.go-back", &["<Alt>Left", "Back"]);
		self.set_accels_for_action("win.go-forward", &["<Alt>Right", "Forward"]);
		self.set_accels_for_action(
			"win.zoom-in",
			&["<Control>plus", "<Control>equal", "<Control>KP_Add"],
		);
		self.set_accels_for_action("win.zoom-out", &["<Control>minus", "<Control>KP_Subtract"]);
		self.set_accels_for_action("win.zoom-reset", &["<Control>0", "<Control>KP_0"]);
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
