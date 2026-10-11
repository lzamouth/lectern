use std::cell::RefCell;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, gio, glib};

use crate::appearance;
use crate::application::LecternApplication;
use crate::document;
use crate::recent;
use crate::search::SearchOptions;
use crate::uri;
use crate::widgets::MarkdownView;

const MARKDOWN_EXTENSIONS: [&str; 5] = ["md", "markdown", "mdown", "mkd", "mkdn"];

/// Where to show a document when opening it.
#[derive(Debug, Clone, PartialEq)]
enum Position {
	Top,
	/// A scrolling position, in pixels.
	Scroll(f64),
	/// The heading with the given anchor.
	Anchor(String),
}

/// How opening a document changes the history.
#[derive(Debug, Clone, Copy)]
enum Navigation {
	New,
	Back,
	Forward,
}

/// A document of the history, with its reading position.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
	file: gio::File,
	scroll: f64,
}

/// Indentation of each heading level in the outline, in pixels.
const OUTLINE_INDENT: i32 = 12;
/// Distance from the top of the view under which a heading is considered read.
const CURRENT_HEADING_MARGIN: f64 = 48.0;

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
		pub outline_button: TemplateChild<gtk::ToggleButton>,
		#[template_child]
		pub split_view: TemplateChild<adw::OverlaySplitView>,
		#[template_child]
		pub outline_list: TemplateChild<gtk::ListBox>,
		#[template_child]
		pub navigation_box: TemplateChild<gtk::Box>,
		#[template_child]
		pub recent_popover: TemplateChild<gtk::Popover>,
		#[template_child]
		pub recent_stack: TemplateChild<gtk::Stack>,
		#[template_child]
		pub recent_list: TemplateChild<gtk::ListBox>,
		#[template_child]
		pub empty_recent_clamp: TemplateChild<adw::Clamp>,
		#[template_child]
		pub empty_recent_list: TemplateChild<gtk::ListBox>,
		#[template_child]
		pub search_button: TemplateChild<gtk::ToggleButton>,
		#[template_child]
		pub search_bar: TemplateChild<gtk::SearchBar>,
		#[template_child]
		pub search_entry: TemplateChild<gtk::SearchEntry>,
		#[template_child]
		pub match_label: TemplateChild<gtk::Label>,
		pub file: RefCell<Option<gio::File>>,
		/// The document shown, to render it again when the preferences change.
		pub document: RefCell<Option<document::Document>>,
		/// Directory against which relative paths of the document are resolved.
		pub base_dir: RefCell<Option<gio::File>>,
		/// Documents to go back and forward to, the next one last.
		pub back: RefCell<Vec<HistoryEntry>>,
		/// Headings listed in the outline, in the order of its rows.
		pub outline: RefCell<Vec<document::OutlineItem>>,
		pub forward: RefCell<Vec<HistoryEntry>>,
		pub rendering_handlers: RefCell<Vec<glib::SignalHandlerId>>,
		pub rerender_source: RefCell<Option<glib::SourceId>>,
		pub file_monitor: RefCell<Option<gio::FileMonitor>>,
		/// Pending reload, delayed until the file stops changing.
		pub reload_source: RefCell<Option<glib::SourceId>>,
		pub recent_handler: RefCell<Option<glib::SignalHandlerId>>,
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
			klass.install_action(
				"win.open-recent",
				Some(glib::VariantTy::STRING),
				|window, _, uri| {
					window.imp().recent_popover.popdown();
					if let Some(uri) = uri.and_then(|uri| uri.get::<String>()) {
						window.open_file(&gio::File::for_uri(&uri));
					}
				},
			);
			klass.install_action("win.copy-source", None, |window, _, _| window.copy_source());
			klass.install_action("win.go-back", None, |window, _, _| window.go_back());
			klass.install_action("win.go-forward", None, |window, _, _| window.go_forward());
			klass.install_action("win.zoom-in", None, |_, _, _| appearance::zoom(true));
			klass.install_action("win.zoom-out", None, |_, _, _| appearance::zoom(false));
			klass.install_action("win.zoom-reset", None, |_, _, _| appearance::reset_zoom());
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
		fn dispose(&self) {
			if let Some(handler) = self.recent_handler.take() {
				gtk::RecentManager::default().disconnect(handler);
			}
			appearance::disconnect_rendering_changed(self.rendering_handlers.take());
			if let Some(source) = self.rerender_source.take() {
				source.remove();
			}
		}

		fn constructed(&self) {
			self.parent_constructed();
			let obj = self.obj();
			obj.load_window_state();
			obj.setup_search();
			obj.action_set_enabled("win.find", false);
			obj.action_set_enabled("win.copy-source", false);
			obj.setup_recent_files();
			obj.setup_drop_target();
			obj.setup_zoom_on_scroll();
			obj.setup_mouse_navigation();
			obj.setup_outline();
			obj.update_navigation();
			let handlers = appearance::connect_rendering_changed(glib::clone!(
				#[weak]
				obj,
				move || obj.schedule_rerender()
			));
			self.rendering_handlers.replace(handlers);
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

	/// Shows `file` in this window. The document shown before, if any, can be
	/// gone back to.
	pub fn open_file(&self, file: &gio::File) {
		self.navigate(file.clone(), Position::Top, Navigation::New);
	}

	/// Shows `file` at `position`, updating the history according to `navigation`
	/// once it is loaded.
	fn navigate(&self, file: gio::File, position: Position, navigation: Navigation) {
		glib::spawn_future_local(glib::clone!(
			#[weak(rename_to = window)]
			self,
			async move {
				let current = window.history_entry();
				if !window.load_file(file, Some(position)).await {
					return;
				}
				let imp = window.imp();
				let mut back = imp.back.borrow_mut();
				let mut forward = imp.forward.borrow_mut();
				match navigation {
					Navigation::New => {
						back.extend(current);
						forward.clear();
					}
					Navigation::Back => {
						back.pop();
						forward.extend(current);
					}
					Navigation::Forward => {
						forward.pop();
						back.extend(current);
					}
				}
				drop((back, forward));
				window.update_navigation();
			}
		));
	}

	/// The document shown and its reading position, to come back to it.
	fn history_entry(&self) -> Option<HistoryEntry> {
		Some(HistoryEntry {
			file: self.file()?,
			scroll: self.imp().markdown_view.scroll_position(),
		})
	}

	fn go_back(&self) {
		let entry = self.imp().back.borrow().last().cloned();
		if let Some(entry) = entry {
			self.navigate(entry.file, Position::Scroll(entry.scroll), Navigation::Back);
		}
	}

	fn go_forward(&self) {
		let entry = self.imp().forward.borrow().last().cloned();
		if let Some(entry) = entry {
			self.navigate(
				entry.file,
				Position::Scroll(entry.scroll),
				Navigation::Forward,
			);
		}
	}

	/// Shows the back and forward buttons when there is a history.
	fn update_navigation(&self) {
		let imp = self.imp();
		let can_go_back = !imp.back.borrow().is_empty();
		let can_go_forward = !imp.forward.borrow().is_empty();
		self.action_set_enabled("win.go-back", can_go_back);
		self.action_set_enabled("win.go-forward", can_go_forward);
		imp.navigation_box
			.set_visible(can_go_back || can_go_forward);
	}

	/// Loads `file`, at `position`, or reloads it when `position` is `None`:
	/// the file is then the current one, the reading position is kept, and
	/// errors are ignored since the file may be in the middle of being saved.
	///
	/// Returns whether the file was shown.
	async fn load_file(&self, file: gio::File, position: Option<Position>) -> bool {
		let imp = self.imp();
		let reload = position.is_none();
		let text = match file.load_contents_future().await {
			Ok((bytes, _etag)) => String::from_utf8_lossy(&bytes).into_owned(),
			Err(_) if reload => return false,
			Err(err) => {
				let message = gettext("Unable to open “{name}”: {error}")
					.replace("{name}", &display_name(&file))
					.replace("{error}", err.message());
				self.show_toast(&message);
				return false;
			}
		};
		// The user may have opened another file meanwhile.
		if reload && self.file().as_ref() != Some(&file) {
			return false;
		}

		let document = document::parse(&text);
		let base_dir = file.parent();
		match position {
			None => {
				self.show_document(&document, base_dir.as_ref(), true);
				imp.document.replace(Some(document));
			}
			Some(position) => {
				self.update_title(&file);
				self.watch(&file);
				recent::add(&file);
				imp.file.replace(Some(file));
				self.show_new_document(document, base_dir, position);
			}
		}
		true
	}

	/// Shows the text read from `stream`, such as the standard input.
	/// Relative paths are relative to `base_dir`.
	pub fn open_stream(&self, stream: gio::InputStream, base_dir: Option<gio::File>) {
		glib::spawn_future_local(glib::clone!(
			#[weak(rename_to = window)]
			self,
			async move {
				let output = gio::MemoryOutputStream::new_resizable();
				let read = output
					.splice_future(
						&stream,
						gio::OutputStreamSpliceFlags::CLOSE_SOURCE
							| gio::OutputStreamSpliceFlags::CLOSE_TARGET,
						glib::Priority::DEFAULT,
					)
					.await;
				if let Err(err) = read {
					window.show_toast(err.message());
					return;
				}
				let bytes = output.steal_as_bytes();
				let text = String::from_utf8_lossy(&bytes);

				let imp = window.imp();
				imp.cancel_reload();
				if let Some(monitor) = imp.file_monitor.take() {
					monitor.cancel();
				}
				imp.file.replace(None);
				let title = gettext("Standard Input");
				imp.window_title.set_title(&title);
				imp.window_title.set_subtitle("");
				window.set_title(Some(&title));
				window.show_new_document(document::parse(&text), base_dir, Position::Top);
			}
		));
	}

	/// Shows a document replacing the previous one, if any.
	fn show_new_document(
		&self,
		document: document::Document,
		base_dir: Option<gio::File>,
		position: Position,
	) {
		let imp = self.imp();
		self.show_document(&document, base_dir.as_ref(), false);
		imp.document.replace(Some(document));
		imp.base_dir.replace(base_dir);
		imp.stack.set_visible_child_name("document");
		self.enable_search();
		// Only scroll to a search match when no position is requested.
		self.search(Some(0), position == Position::Top);
		match position {
			Position::Top => {}
			Position::Scroll(scroll) => imp.markdown_view.scroll_to_position(scroll),
			Position::Anchor(anchor) => {
				imp.markdown_view.scroll_to_anchor(&anchor);
			}
		}

		#[cfg(debug_assertions)]
		{
			if let Ok(query) = std::env::var("LECTERN_SNAPSHOT_SEARCH") {
				imp.search_bar.set_search_mode(true);
				imp.search_entry.set_text(&query);
			}
			let window = self.downgrade();
			crate::devtools::snapshot_if_requested(self, move |step| {
				let Some(window) = window.upgrade() else {
					return;
				};
				if let Some(url) = step.strip_prefix("link:") {
					window.activate_link(url);
				} else if let Some((start, end)) = step
					.strip_prefix("select:")
					.and_then(|range| range.split_once('-'))
					.and_then(|(start, end)| Some((start.parse().ok()?, end.parse().ok()?)))
				{
					let buffer = window.imp().markdown_view.buffer();
					buffer.select_range(&buffer.iter_at_offset(start), &buffer.iter_at_offset(end));
				}
			});
		}
	}

	/// Whether the window shows a document.
	pub fn has_document(&self) -> bool {
		self.imp().document.borrow().is_some()
	}

	fn setup_recent_files(&self) {
		let imp = self.imp();
		let handler = gtk::RecentManager::default().connect_changed(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |_| window.update_recent_files()
		));
		imp.recent_handler.replace(Some(handler));

		self.update_recent_files();
	}

	fn update_recent_files(&self) {
		let imp = self.imp();
		let files = recent::files();
		for list in [&*imp.recent_list, &*imp.empty_recent_list] {
			list.remove_all();
			for file in &files {
				let row = adw::ActionRow::builder()
					.title(glib::markup_escape_text(&display_name(file)))
					.subtitle(glib::markup_escape_text(
						&file
							.parent()
							.map(|dir| display_dir(&dir))
							.unwrap_or_default(),
					))
					.activatable(true)
					.action_name("win.open-recent")
					.action_target(&file.uri().to_variant())
					.build();
				list.append(&row);
			}
		}
		imp.recent_stack
			.set_visible_child_name(if files.is_empty() { "empty" } else { "list" });
		imp.empty_recent_clamp.set_visible(!files.is_empty());
	}

	fn setup_outline(&self) {
		let imp = self.imp();
		self.add_action(&gio::PropertyAction::new(
			"show-outline",
			&*imp.split_view,
			"show-sidebar",
		));
		self.settings()
			.bind("show-outline", &*imp.split_view, "show-sidebar")
			.build();

		let placeholder = gtk::Label::builder()
			.label(gettext("No Headings"))
			.margin_top(12)
			.margin_bottom(12)
			.css_classes(["dimmed"])
			.build();
		imp.outline_list.set_placeholder(Some(&placeholder));

		imp.outline_list.connect_row_activated(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |_, row| {
				let imp = window.imp();
				let anchor = imp
					.outline
					.borrow()
					.get(row.index() as usize)
					.map(|item| item.anchor.clone());
				if let Some(anchor) = anchor {
					imp.markdown_view.scroll_to_anchor(&anchor);
				}
				// The outline covers the document when the window is narrow.
				if imp.split_view.is_collapsed() {
					imp.split_view.set_show_sidebar(false);
				}
			}
		));

		if let Some(adjustment) = imp.markdown_view.vadjustment() {
			adjustment.connect_value_changed(glib::clone!(
				#[weak(rename_to = window)]
				self,
				move |_| window.highlight_current_heading()
			));
			// The positions of the headings are only known once laid out, which
			// changes the size of the document.
			adjustment.connect_changed(glib::clone!(
				#[weak(rename_to = window)]
				self,
				move |_| window.highlight_current_heading()
			));
		}
	}

	fn update_outline(&self, document: &document::Document) {
		let imp = self.imp();
		let items = document.outline();
		imp.outline_list.remove_all();
		// Indent relative to the highest level of the document.
		let top_level = items.iter().map(|item| item.level).min().unwrap_or(1);
		for item in &items {
			let label = gtk::Label::builder()
				.label(&item.text)
				.xalign(0.0)
				.ellipsize(gtk::pango::EllipsizeMode::End)
				.tooltip_text(&item.text)
				.margin_start(i32::from(item.level - top_level) * OUTLINE_INDENT)
				.build();
			if item.level == top_level {
				label.add_css_class("heading");
			}
			imp.outline_list.append(&label);
		}
		imp.outline.replace(items);
		self.highlight_current_heading();
	}

	/// Selects the heading of the section being read in the outline.
	fn highlight_current_heading(&self) {
		let imp = self.imp();
		let view = &imp.markdown_view;
		// A heading close to the top of the view starts the current section.
		let top = view.scroll_position() + CURRENT_HEADING_MARGIN;
		let current = imp
			.outline
			.borrow()
			.iter()
			.map(|item| view.anchor_position(&item.anchor))
			.take_while(|position| position.is_some_and(|position| position <= top))
			.count();
		match current.checked_sub(1) {
			Some(index) => {
				let row = imp.outline_list.row_at_index(index as i32);
				if imp.outline_list.selected_row() != row {
					imp.outline_list.select_row(row.as_ref());
				}
			}
			None => imp.outline_list.unselect_all(),
		}
	}

	/// The back and forward buttons of the mouse navigate the history.
	fn setup_mouse_navigation(&self) {
		let click = gtk::GestureClick::new();
		click.set_button(0);
		click.connect_pressed(|gesture, _, _, _| {
			let action = match gesture.current_button() {
				8 => "win.go-back",
				9 => "win.go-forward",
				_ => return,
			};
			gesture.set_state(gtk::EventSequenceState::Claimed);
			if let Some(widget) = gesture.widget() {
				let _ = widget.activate_action(action, None);
			}
		});
		self.add_controller(click);
	}

	/// Ctrl+scroll zooms.
	fn setup_zoom_on_scroll(&self) {
		let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
		scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
		scroll.connect_scroll(|controller, _, dy| {
			if !controller
				.current_event_state()
				.contains(gdk::ModifierType::CONTROL_MASK)
				|| dy == 0.0
			{
				return glib::Propagation::Proceed;
			}
			appearance::zoom(dy < 0.0);
			glib::Propagation::Stop
		});
		self.imp().markdown_view.add_controller(scroll);
	}

	/// Opens files dropped on the window: the first one in this window, the
	/// others in new windows.
	fn setup_drop_target(&self) {
		let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
		target.connect_drop(glib::clone!(
			#[weak(rename_to = window)]
			self,
			#[upgrade_or]
			false,
			move |_, value, _, _| {
				let Ok(files) = value.get::<gdk::FileList>() else {
					return false;
				};
				let mut files = files.files().into_iter();
				let Some(first) = files.next() else {
					return false;
				};
				window.open_file(&first);
				if let Some(app) = window.application().and_downcast::<LecternApplication>() {
					for file in files {
						let other = LecternWindow::new(&app);
						other.open_file(&file);
						other.present();
					}
				}
				true
			}
		));
		self.add_controller(target);
	}

	/// Renders `document`, whose relative paths are relative to `base_dir`.
	/// When `keep_state` is set, the document replaces a version of itself:
	/// the reading position, the column widths and the selected search match
	/// are kept.
	fn show_document(
		&self,
		document: &document::Document,
		base_dir: Option<&gio::File>,
		keep_state: bool,
	) {
		let view = &self.imp().markdown_view;
		let state = keep_state.then(|| view.view_state());
		let current_match = view.match_position().0;
		let margin = appearance::margin();
		view.set_left_margin(margin);
		view.set_right_margin(margin);
		view.set_document(document, base_dir);
		self.update_outline(document);
		if let Some(state) = state {
			view.restore_view_state(&state);
			self.search(Some(current_match.unwrap_or_default()), false);
		}
	}

	/// Renders the document again once the new style is applied, after a
	/// change of the preferences. Several changes are coalesced.
	fn schedule_rerender(&self) {
		let imp = self.imp();
		if imp.rerender_source.borrow().is_some() {
			return;
		}
		let source = glib::idle_add_local_once(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move || {
				window.imp().rerender_source.take();
				window.rerender();
			}
		));
		imp.rerender_source.replace(Some(source));
	}

	fn rerender(&self) {
		let imp = self.imp();
		let Some(document) = imp.document.borrow().clone() else {
			return;
		};
		let base_dir = imp.base_dir.borrow().clone();
		self.show_document(&document, base_dir.as_ref(), true);
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
							window.load_file(file, None).await;
						});
					}
				}
			),
		);
		imp.reload_source.replace(Some(source));
	}

	/// Copies the Markdown source of the selected blocks, or of the whole
	/// document without a selection.
	fn copy_source(&self) {
		let imp = self.imp();
		let Some(document) = imp.document.borrow().clone() else {
			return;
		};
		let lines = imp
			.markdown_view
			.selected_leaves()
			.and_then(|(first, last)| {
				let (first_line, _) = *document.leaf_lines.get(first)?;
				let (_, last_line) = *document.leaf_lines.get(last)?;
				Some((first_line, last_line))
			});
		let (text, message) = match lines {
			Some((first, last)) => (
				document.source_lines(first, last),
				gettext("Markdown source of the selection copied"),
			),
			None => (
				document.source.clone(),
				gettext("Markdown source of the document copied"),
			),
		};
		self.clipboard().set_text(&text);
		self.show_toast(&message);
	}

	/// Makes the search and the outline available, once a document is shown.
	fn enable_search(&self) {
		let imp = self.imp();
		self.action_set_enabled("win.copy-source", true);
		imp.search_button.set_visible(true);
		imp.outline_button.set_visible(true);
		// Typing in the window starts searching.
		imp.search_bar.set_key_capture_widget(Some(self));
		self.action_set_enabled("win.find", true);
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

		// The entry is not a direct child of the bar: connect it for typing
		// to start searching, and Escape to stop.
		imp.search_bar.connect_entry(&*imp.search_entry);

		// Typing starts searching, also after clicking the search button.
		imp.search_button.connect_toggled(glib::clone!(
			#[weak(rename_to = window)]
			self,
			move |button| {
				if button.is_active() {
					window.imp().search_entry.grab_focus();
				}
			}
		));

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
		let base_dir = self.imp().base_dir.borrow().clone();
		let Some(target) = uri::local_file(url, base_dir.as_ref()) else {
			return;
		};

		if is_markdown(&target) {
			let position = url.split_once('#').map_or(Position::Top, |(_, anchor)| {
				Position::Anchor(anchor.to_owned())
			});
			self.navigate(target, position, Navigation::New);
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

	fn settings(&self) -> gio::Settings {
		appearance::settings()
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
