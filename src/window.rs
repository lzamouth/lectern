use std::cell::{OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::application::LecternApplication;
use crate::config;

mod imp {
	use super::*;

	#[derive(Debug, Default, gtk::CompositeTemplate)]
	#[template(resource = "/com/skunkcrafts/Lectern/ui/window.ui")]
	pub struct LecternWindow {
		#[template_child]
		pub toast_overlay: TemplateChild<adw::ToastOverlay>,
		#[template_child]
		pub window_title: TemplateChild<adw::WindowTitle>,
		#[template_child]
		pub stack: TemplateChild<gtk::Stack>,
		#[template_child]
		pub text_view: TemplateChild<gtk::TextView>,
		pub file: RefCell<Option<gio::File>>,
		pub settings: OnceCell<gio::Settings>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for LecternWindow {
		const NAME: &'static str = "LecternWindow";
		type Type = super::LecternWindow;
		type ParentType = adw::ApplicationWindow;

		fn class_init(klass: &mut Self::Class) {
			klass.bind_template();
			klass.install_action_async("win.open", None, |window, _, _| async move {
				window.choose_file().await;
			});
		}

		fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
			obj.init_template();
		}
	}

	impl ObjectImpl for LecternWindow {
		fn constructed(&self) {
			self.parent_constructed();
			self.obj().load_window_state();
		}
	}

	impl WidgetImpl for LecternWindow {}

	impl WindowImpl for LecternWindow {
		fn close_request(&self) -> glib::Propagation {
			self.obj().save_window_state();
			self.parent_close_request()
		}
	}

	impl ApplicationWindowImpl for LecternWindow {}
	impl AdwApplicationWindowImpl for LecternWindow {}
}

glib::wrapper! {
	pub struct LecternWindow(ObjectSubclass<imp::LecternWindow>)
		@extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
		@implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
			gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl LecternWindow {
	pub fn new(app: &LecternApplication) -> Self {
		glib::Object::builder().property("application", app).build()
	}

	/// The document currently shown, if any.
	pub fn file(&self) -> Option<gio::File> {
		self.imp().file.borrow().clone()
	}

	pub fn open_file(&self, file: &gio::File) {
		let file = file.clone();
		glib::spawn_future_local(glib::clone!(
			#[weak(rename_to = window)]
			self,
			async move {
				window.load_file(file).await;
			}
		));
	}

	async fn load_file(&self, file: gio::File) {
		let imp = self.imp();
		match file.load_contents_future().await {
			Ok((bytes, _etag)) => {
				let text = String::from_utf8_lossy(&bytes);
				imp.text_view.buffer().set_text(&text);
				self.update_title(&file);
				imp.file.replace(Some(file));
				imp.stack.set_visible_child_name("document");
			}
			Err(err) => {
				let message = gettext("Unable to open “{name}”: {error}")
					.replace("{name}", &display_name(&file))
					.replace("{error}", err.message());
				self.show_toast(&message);
			}
		}
	}

	async fn choose_file(&self) {
		let filter = gtk::FileFilter::new();
		filter.set_name(Some(&gettext("Markdown Files")));
		filter.add_mime_type("text/markdown");
		filter.add_suffix("md");
		filter.add_suffix("markdown");

		let filters = gio::ListStore::new::<gtk::FileFilter>();
		filters.append(&filter);

		let dialog = gtk::FileDialog::builder()
			.title(gettext("Open Markdown File"))
			.modal(true)
			.filters(&filters)
			.default_filter(&filter)
			.build();

		match dialog.open_future(Some(self)).await {
			Ok(file) => self.open_file(&file),
			Err(err) if err.matches(gtk::DialogError::Dismissed) => {}
			Err(err) => self.show_toast(err.message()),
		}
	}

	fn update_title(&self, file: &gio::File) {
		let name = display_name(file);
		let imp = self.imp();
		imp.window_title.set_title(&name);
		imp.window_title.set_subtitle(
			&file
				.parent()
				.map(|dir| display_dir(&dir))
				.unwrap_or_default(),
		);
		self.set_title(Some(&name));
	}

	fn show_toast(&self, message: &str) {
		self.imp().toast_overlay.add_toast(adw::Toast::new(message));
	}

	fn settings(&self) -> &gio::Settings {
		self.imp()
			.settings
			.get_or_init(|| gio::Settings::new(config::APP_ID))
	}

	fn load_window_state(&self) {
		let settings = self.settings();
		self.set_default_size(settings.int("window-width"), settings.int("window-height"));
		if settings.boolean("is-maximized") {
			self.maximize();
		}
	}

	fn save_window_state(&self) {
		let settings = self.settings();
		let (width, height) = self.default_size();
		// Errors only happen if the keys are not writable, nothing to do then.
		let _ = settings.set_int("window-width", width);
		let _ = settings.set_int("window-height", height);
		let _ = settings.set_boolean("is-maximized", self.is_maximized());
	}
}

fn display_name(file: &gio::File) -> String {
	file.basename()
		.map(|name| name.to_string_lossy().into_owned())
		.unwrap_or_else(|| file.uri().to_string())
}

/// Human-readable directory, with the home directory abbreviated to `~`.
fn display_dir(dir: &gio::File) -> String {
	let Some(path) = dir.path() else {
		return dir.parse_name().to_string();
	};
	match path.strip_prefix(glib::home_dir()) {
		Ok(relative) if relative.as_os_str().is_empty() => "~".to_owned(),
		Ok(relative) => format!("~/{}", relative.display()),
		Err(_) => path.display().to_string(),
	}
}
