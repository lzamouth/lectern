use std::cell::{OnceCell, RefCell};
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::application::LecternApplication;
use crate::config;
use crate::document;
use crate::search::SearchOptions;
use crate::uri;
use crate::widgets::MarkdownView;

const MARKDOWN_EXTENSIONS: [&str; 5] = ["md", "markdown", "mdown", "mkd", "mkdn"];

/// Delay before reloading a changed file, to let editors finish saving it.
const RELOAD_DELAY: Duration = Duration::from_millis(250);

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
		#[template_child]
		pub search_bar: TemplateChild<gtk::SearchBar>,
		#[template_child]
		pub search_entry: TemplateChild<gtk::SearchEntry>,
		#[template_child]
		pub match_label: TemplateChild<gtk::Label>,
		pub file: RefCell<Option<gio::File>>,
		pub file_monitor: RefCell<Option<gio::FileMonitor>>,
		/// Pending reload, delayed until the file stops changing.
		pub reload_source: RefCell<Option<glib::SourceId>>,
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
			klass.install_action("win.find", None, |window, _, _| {
				let imp = window.imp();
				imp.search_bar.set_search_mode(true);
				imp.search_entry.grab_focus();
				imp.search_entry.select_region(0, -1);
			});
			klass.install_action("win.find-next", None, |window, _, _| {
				window.select_next_match(false);
			});
			klass.install_action("win.find-previous", None, |window, _, _| {
				window.select_next_match(true);
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
			obj.setup_search();
			self.markdown_view.connect_link_activated(glib::clone!(
				#[weak]
				obj,
				move |_, url| obj.activate_link(url)
			));
		}
	}

	impl WidgetImpl for LecternWindow {}

	impl LecternWindow {
		pub(super) fn cancel_reload(&self) {
			if let Some(source) = self.reload_source.take() {
				source.remove();
			}
		}
	}

	impl WindowImpl for LecternWindow {
		fn close_request(&self) -> glib::Propagation {
			self.cancel_reload();
			if let Some(monitor) = self.file_monitor.take() {
				monitor.cancel();
			}
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
				window.load_file(file, false).await;
			}
		));
	}

	/// Loads `file`. When `reload` is set, the file is the current one: the
	/// reading position is kept, and errors are ignored since the file may be
	/// in the middle of being saved.
	async fn load_file(&self, file: gio::File, reload: bool) {
		let imp = self.imp();
		let text = match file.load_contents_future().await {
			Ok((bytes, _etag)) => String::from_utf8_lossy(&bytes).into_owned(),
			Err(_) if reload => return,
			Err(err) => {
				let message = gettext("Unable to open “{name}”: {error}")
					.replace("{name}", &display_name(&file))
					.replace("{error}", err.message());
				self.show_toast(&message);
				return;
			}
		};
		// The user may have opened another file meanwhile.
		if reload && self.file().as_ref() != Some(&file) {
			return;
		}

		let view = &imp.markdown_view;
		let state = reload.then(|| view.view_state());
		let current_match = view.match_position().0;
		view.set_document(&document::parse(&text), file.parent().as_ref());

		if let Some(state) = state {
			view.restore_view_state(&state);
			self.search(Some(current_match.unwrap_or_default()), false);
		} else {
			self.update_title(&file);
			self.watch(&file);
			imp.file.replace(Some(file));
			imp.stack.set_visible_child_name("document");
			self.search(Some(0), true);
		}

		#[cfg(debug_assertions)]
		if !reload {
			if let Ok(query) = std::env::var("LECTERN_SNAPSHOT_SEARCH") {
				imp.search_bar.set_search_mode(true);
				imp.search_entry.set_text(&query);
			}
			crate::devtools::snapshot_if_requested(self);
		}
	}

	/// Reloads the document when its file changes.
	fn watch(&self, file: &gio::File) {
		let imp = self.imp();
		imp.cancel_reload();
		if let Some(monitor) = imp.file_monitor.take() {
			monitor.cancel();
		}
		// Editors often save by renaming a new file over the old one: watch moves.
		let Ok(monitor) =
			file.monitor_file(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
		else {
			return;
		};
		monitor.connect_changed(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |_, _, _, event| {
				if !matches!(
					event,
					gio::FileMonitorEvent::AttributeChanged
						| gio::FileMonitorEvent::PreUnmount
						| gio::FileMonitorEvent::Unmounted
				) {
					window.schedule_reload();
				}
			}
		));
		imp.file_monitor.replace(Some(monitor));
	}

	fn schedule_reload(&self) {
		let imp = self.imp();
		imp.cancel_reload();
		let source = glib::timeout_add_local_once(
			RELOAD_DELAY,
			glib::clone!(
				#[weak(rename_to = window)]
				self,
				move || {
					window.imp().reload_source.take();
					if let Some(file) = window.file() {
						glib::spawn_future_local(async move {
							window.load_file(file, true).await;
						});
					}
				}
			),
		);
		imp.reload_source.replace(Some(source));
	}

	fn setup_search(&self) {
		let imp = self.imp();
		let settings = self.settings();
		for key in ["search-case-sensitive", "search-whole-words"] {
			self.add_action(&settings.create_action(key));
			settings.connect_changed(
				Some(key),
				glib::clone!(
					#[weak(rename_to = window)]
					self,
					move |_, _| window.search(Some(0), true)
				),
			);
		}

		imp.search_entry.connect_search_changed(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |_| window.search(Some(0), true)
		));
		imp.search_entry.connect_activate(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |_| window.select_next_match(false)
		));
		imp.search_entry.connect_next_match(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |_| window.select_next_match(false)
		));
		imp.search_entry.connect_previous_match(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |_| window.select_next_match(true)
		));

		// Shift+Enter selects the previous match.
		let shortcuts = gtk::ShortcutController::new();
		shortcuts.add_shortcut(gtk::Shortcut::new(
			gtk::ShortcutTrigger::parse_string("<Shift>Return"),
			Some(gtk::NamedAction::new("win.find-previous")),
		));
		imp.search_entry.add_controller(shortcuts);

		imp.search_bar
			.connect_search_mode_enabled_notify(glib::clone!(
				#[weak(rename_to = window)]
				self,
				move |bar| {
					if bar.is_search_mode() {
						window.search(Some(0), true);
					} else {
						window.imp().markdown_view.clear_search();
						window.update_match_label();
					}
				}
			));
	}

	/// Searches the text of the search entry in the document, and selects the
	/// match at index `select` (or the last one), scrolling to it if `reveal`.
	fn search(&self, select: Option<usize>, reveal: bool) {
		let imp = self.imp();
		if !imp.search_bar.is_search_mode() {
			return;
		}
		let settings = self.settings();
		let options = SearchOptions {
			case_sensitive: settings.boolean("search-case-sensitive"),
			whole_words: settings.boolean("search-whole-words"),
		};
		let count = imp.markdown_view.search(&imp.search_entry.text(), options);
		if let Some(index) = select
			&& count > 0
		{
			imp.markdown_view.select_match(index.min(count - 1), reveal);
		}
		self.update_match_label();
	}

	fn select_next_match(&self, backward: bool) {
		self.imp().markdown_view.select_next_match(backward);
		self.update_match_label();
	}

	fn update_match_label(&self) {
		let imp = self.imp();
		let (current, count) = imp.markdown_view.match_position();
		let searching = imp.search_bar.is_search_mode() && !imp.search_entry.text().is_empty();
		let text = match current {
			Some(index) if searching => gettext("{current} of {count}")
				.replace("{current}", &(index + 1).to_string())
				.replace("{count}", &count.to_string()),
			None if searching => gettext("No results"),
			_ => String::new(),
		};
		imp.match_label.set_label(&text);
		if searching && count == 0 {
			imp.search_entry.add_css_class("error");
		} else {
			imp.search_entry.remove_css_class("error");
		}
		self.action_set_enabled("win.find-next", count > 0);
		self.action_set_enabled("win.find-previous", count > 0);
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
