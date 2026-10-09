use std::cell::{OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::application::LecternApplication;
use crate::config;
use crate::document;
use crate::uri;
use crate::widgets::MarkdownView;

const MARKDOWN_EXTENSIONS: [&str; 5] = ["md", "markdown", "mdown", "mkd", "mkdn"];

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
		pub markdown_view: TemplateChild<MarkdownView>,
		pub file: RefCell<Option<gio::File>>,
		pub settings: OnceCell<gio::Settings>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for LecternWindow {
		const NAME: &'static str = "LecternWindow";
		type Type = super::LecternWindow;
		type ParentType = adw::ApplicationWindow;

		fn class_init(klass: &mut Self::Class) {
			MarkdownView::ensure_type();
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
			let obj = self.obj();
			obj.load_window_state();
			self.markdown_view.connect_link_activated(glib::clone!(
				#[weak]
				obj,
				move |_, url| obj.activate_link(url)
			));
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
				imp.markdown_view
					.set_document(&document::parse(&text), file.parent().as_ref());
				self.update_title(&file);
				imp.file.replace(Some(file));
				imp.stack.set_visible_child_name("document");
				#[cfg(debug_assertions)]
				crate::devtools::snapshot_if_requested(self);
			}
			Err(err) => {
				let message = gettext("Unable to open “{name}”: {error}")
					.replace("{name}", &display_name(&file))
					.replace("{error}", err.message());
				self.show_toast(&message);
			}
		}
	}

	fn activate_link(&self, url: &str) {
		if let Some(anchor) = url.strip_prefix('#') {
			self.imp().markdown_view.scroll_to_anchor(anchor);
			return;
		}

		if glib::Uri::peek_scheme(url).is_some_and(|scheme| scheme != "file") {
			self.launch(gtk::UriLauncher::new(url).launch_future(Some(self)));
			return;
		}
		let base_dir = self.file().and_then(|file| file.parent());
		let Some(target) = uri::local_file(url, base_dir.as_ref()) else {
			return;
		};

		if is_markdown(&target) {
			self.open_file(&target);
		} else {
			self.launch(gtk::FileLauncher::new(Some(&target)).launch_future(Some(self)));
		}
	}

	/// Runs a launcher, reporting failures in a toast.
	fn launch(&self, future: impl std::future::Future<Output = Result<(), glib::Error>> + 'static) {
		glib::spawn_future_local(glib::clone!(
			#[weak(rename_to = window)]
			self,
			async move {
				if let Err(err) = future.await
					&& !err.matches(gtk::DialogError::Dismissed)
				{
					window.show_toast(err.message());
				}
			}
		));
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

fn is_markdown(file: &gio::File) -> bool {
	file.path()
		.and_then(|path| {
			path.extension()
				.map(|ext| ext.to_string_lossy().to_ascii_lowercase())
		})
		.is_some_and(|ext| MARKDOWN_EXTENSIONS.contains(&ext.as_str()))
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
