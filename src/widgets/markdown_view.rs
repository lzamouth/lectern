//! A read-only text view that renders a [`Document`] and handles its links.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::OnceLock;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib::subclass::Signal;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib, graphene, pango};

use super::{CodeBlock, Image, Table};
use crate::appearance;
use crate::document::{AlertKind, Block, Document, Footnote, List, ListKind, Span};
use crate::search::{self, SearchOptions};
use crate::uri;

const HEADING_SCALES: [f64; 6] = [2.0, 1.6, 1.35, 1.17, 1.0, 0.9];
const HEADING_TAGS: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];

/// Vertical space after a block, in pixels.
const BLOCK_SPACING: i32 = 12;
/// Vertical space between the items of a tight list, in pixels.
const TIGHT_SPACING: i32 = 4;
const SPACINGS: [i32; 2] = [BLOCK_SPACING, TIGHT_SPACING];

/// Horizontal indentation of nested blocks, in pixels.
const LIST_INDENT: i32 = 28;
const QUOTE_INDENT: i32 = 20;
const QUOTE_BAR_WIDTH: f32 = 3.0;
const RULE_HEIGHT: f32 = 1.0;

const BULLETS: [&str; 3] = ["•", "◦", "▪"];

/// Text range of a link in the buffer, as character offsets.
#[derive(Debug)]
struct Link {
	start: i32,
	end: i32,
	url: String,
}

/// A widget embedded in the text at a child anchor.
#[derive(Debug)]
struct Embedded {
	/// Buffer offset of the anchor.
	offset: i32,
	indent: i32,
	widget: gtk::Widget,
	/// Text copied in place of the widget.
	text: String,
}

/// State of the view to keep when the document is reloaded.
#[derive(Debug, Default)]
pub struct ViewState {
	/// Vertical scrolling position, in pixels.
	scroll: f64,
	/// Widths of the table columns, for the tables resized by the user.
	table_widths: Vec<Option<Vec<i32>>>,
}

/// A search match in the buffer of a text view, as character offsets.
#[derive(Debug, Clone)]
struct Match {
	view: gtk::TextView,
	start: i32,
	end: i32,
	/// Offset of the anchor of the embedded widget holding `view`, if any.
	anchor: Option<i32>,
}

const MATCH_TAG: &str = "search-match";
const CURRENT_MATCH_TAG: &str = "search-current-match";
/// Maximum number of frames spent scrolling to a match in an embedded widget.
const MAX_REVEAL_FRAMES: u32 = 60;
/// Maximum number of frames spent restoring the scrolling position.
const MAX_RESTORE_FRAMES: u32 = 60;
/// Mark used to scroll to the selected match.
const REVEAL_MARK: &str = "search-reveal";

/// Graphics drawn below the text. Offsets are buffer character offsets.
#[derive(Debug)]
enum Decoration {
	/// Vertical bar on the left of the lines in `start..end`.
	QuoteBar {
		indent: i32,
		spacing: i32,
		start: i32,
		end: i32,
		/// Colors the bar for alerts.
		alert: Option<AlertKind>,
	},
	/// Horizontal line in the middle of the line at `offset`.
	Rule {
		indent: i32,
		spacing: i32,
		offset: i32,
	},
}

mod imp {
	use super::*;

	#[derive(Debug, Default)]
	pub struct MarkdownView {
		pub(super) links: RefCell<Vec<Link>>,
		pub(super) anchors: RefCell<HashMap<String, gtk::TextMark>>,
		pub(super) decorations: RefCell<Vec<Decoration>>,
		pub(super) embedded: RefCell<Vec<Embedded>>,
		pub(super) matches: RefCell<Vec<Match>>,
		pub(super) current_match: Cell<Option<usize>>,
		/// Width for which embedded widgets were last sized.
		pub(super) embedded_width: Cell<i32>,
		pub(super) style_handlers: RefCell<Vec<glib::SignalHandlerId>>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for MarkdownView {
		const NAME: &'static str = "LecternMarkdownView";
		type Type = super::MarkdownView;
		type ParentType = gtk::TextView;
	}

	impl ObjectImpl for MarkdownView {
		fn signals() -> &'static [Signal] {
			static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
			SIGNALS.get_or_init(|| {
				vec![
					Signal::builder("link-activated")
						.param_types([String::static_type()])
						.build(),
				]
			})
		}

		fn constructed(&self) {
			self.parent_constructed();
			self.obj().setup();
		}

		fn dispose(&self) {
			let style_manager = adw::StyleManager::default();
			for handler in self.style_handlers.take() {
				style_manager.disconnect(handler);
			}
		}
	}

	impl WidgetImpl for MarkdownView {
		fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
			let (minimum, natural, minimum_baseline, natural_baseline) =
				self.parent_measure(orientation, for_size);
			if orientation == gtk::Orientation::Vertical {
				return (minimum, natural, minimum_baseline, natural_baseline);
			}
			// Embedded widgets are resized to the width of the view after each
			// allocation: they must not prevent the view from shrinking.
			let obj = self.obj();
			let minimum = minimum.min(obj.left_margin() + obj.right_margin() + 1);
			(minimum, natural.max(minimum), -1, -1)
		}

		fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
			self.parent_size_allocate(width, height, baseline);
			if width != self.embedded_width.get() {
				self.embedded_width.set(width);
				// Resizing children while allocating would require another
				// allocation: do it once this one is over.
				let obj = self.obj();
				glib::idle_add_local_once(glib::clone!(
					#[weak]
					obj,
					move || obj.resize_embedded()
				));
			}
		}
	}

	impl TextViewImpl for MarkdownView {
		fn copy_clipboard(&self) {
			let obj = self.obj();
			if let Some(text) = obj.selected_text() {
				obj.clipboard().set_text(&text);
			}
		}

		fn snapshot_layer(&self, layer: gtk::TextViewLayer, snapshot: gtk::Snapshot) {
			self.parent_snapshot_layer(layer, snapshot.clone());
			if layer == gtk::TextViewLayer::BelowText {
				self.obj().draw_decorations(&snapshot);
			}
		}
	}
}

glib::wrapper! {
	pub struct MarkdownView(ObjectSubclass<imp::MarkdownView>)
		@extends gtk::TextView, gtk::Widget,
		@implements gtk::Accessible, gtk::AccessibleText, gtk::Buildable,
			gtk::ConstraintTarget, gtk::Scrollable;
}

impl MarkdownView {
	/// Shows `document`. Relative paths are resolved against `base_dir`.
	pub fn set_document(&self, document: &Document, base_dir: Option<&gio::File>) {
		let buffer = self.buffer();
		self.update_code_font();
		// The matches refer to the views being destroyed.
		self.imp().matches.take();
		self.imp().current_match.set(None);
		buffer.set_text("");
		let mut renderer = Renderer {
			view: self,
			base_dir,
			buffer: &buffer,
			links: Vec::new(),
			anchors: HashMap::new(),
			decorations: Vec::new(),
			embedded: Vec::new(),
		};
		renderer.blocks(&document.blocks, Context::default());
		renderer.finish();

		let imp = self.imp();
		imp.links.replace(renderer.links);
		imp.decorations.replace(renderer.decorations);
		imp.embedded.replace(renderer.embedded);
		self.resize_embedded();
		let old_anchors = imp.anchors.replace(renderer.anchors);
		for mark in old_anchors.values() {
			buffer.delete_mark(mark);
		}
		buffer.place_cursor(&buffer.start_iter());
	}

	/// Creates a view showing a single paragraph, for a table cell.
	pub fn new_cell(
		content: &[Span],
		justification: gtk::Justification,
		base_dir: Option<&gio::File>,
	) -> Self {
		let view: Self = glib::Object::new();
		view.set_justification(justification);
		view.set_document(
			&Document {
				blocks: vec![Block::Paragraph(content.to_vec())],
			},
			base_dir,
		);
		view
	}

	/// A callback emitting `link-activated` on this view, for embedded widgets.
	fn link_callback(&self) -> impl Fn(&str) + Clone + 'static {
		let view = self.downgrade();
		move |url| {
			if let Some(view) = view.upgrade() {
				view.emit_by_name::<()>("link-activated", &[&url]);
			}
		}
	}

	/// Sets the horizontal and vertical margins around the text.
	pub fn set_margins(&self, horizontal: i32, vertical: i32) {
		self.set_left_margin(horizontal);
		self.set_right_margin(horizontal);
		self.set_top_margin(vertical);
		self.set_bottom_margin(vertical);
	}

	/// Highlights the occurrences of `query` in the document, including code
	/// blocks and tables. Returns their count.
	pub fn search(&self, query: &str, options: SearchOptions) -> usize {
		self.clear_search();

		// Matches in embedded widgets are sorted at the offset of their anchor,
		// after any match of the main text at the same offset.
		let mut keyed: Vec<((i32, usize), Match)> = Vec::new();
		let main_view: gtk::TextView = self.clone().upcast();
		for found in find_in_view(&main_view, query, options, None) {
			keyed.push(((found.start, 0), found));
		}
		for embedded in self.imp().embedded.borrow().iter() {
			for (index, view) in embedded_text_views(&embedded.widget).iter().enumerate() {
				for found in find_in_view(view, query, options, Some(embedded.offset)) {
					keyed.push(((embedded.offset, index + 1), found));
				}
			}
		}
		keyed.sort_by_key(|(key, _)| *key);
		let matches: Vec<Match> = keyed.into_iter().map(|(_, found)| found).collect();

		for found in &matches {
			let buffer = found.view.buffer();
			ensure_search_tags(&buffer);
			buffer.apply_tag_by_name(
				MATCH_TAG,
				&buffer.iter_at_offset(found.start),
				&buffer.iter_at_offset(found.end),
			);
		}
		let count = matches.len();
		self.imp().matches.replace(matches);
		count
	}

	/// Removes the search highlighting.
	pub fn clear_search(&self) {
		let imp = self.imp();
		imp.current_match.set(None);
		let mut cleared: Vec<gtk::TextBuffer> = Vec::new();
		for found in imp.matches.take() {
			let buffer = found.view.buffer();
			if cleared.contains(&buffer) {
				continue;
			}
			let (start, end) = buffer.bounds();
			buffer.remove_tag_by_name(MATCH_TAG, &start, &end);
			buffer.remove_tag_by_name(CURRENT_MATCH_TAG, &start, &end);
			cleared.push(buffer);
		}
	}

	/// The index of the selected match, and the number of matches.
	pub fn match_position(&self) -> (Option<usize>, usize) {
		let imp = self.imp();
		(imp.current_match.get(), imp.matches.borrow().len())
	}

	/// Selects the next match, or the previous one if `backward`, wrapping around.
	pub fn select_next_match(&self, backward: bool) {
		let (current, count) = self.match_position();
		if count == 0 {
			return;
		}
		let index = match (current, backward) {
			(None, _) => 0,
			(Some(index), false) => (index + 1) % count,
			(Some(index), true) => (index + count - 1) % count,
		};
		self.select_match(index, true);
	}

	/// Selects a match, and scrolls to it if `reveal`.
	pub fn select_match(&self, index: usize, reveal: bool) {
		let imp = self.imp();
		let matches = imp.matches.borrow();
		if let Some(previous) = imp.current_match.get().and_then(|index| matches.get(index)) {
			let buffer = previous.view.buffer();
			buffer.remove_tag_by_name(
				CURRENT_MATCH_TAG,
				&buffer.iter_at_offset(previous.start),
				&buffer.iter_at_offset(previous.end),
			);
		}
		let Some(found) = matches.get(index) else {
			return;
		};
		let buffer = found.view.buffer();
		buffer.apply_tag_by_name(
			CURRENT_MATCH_TAG,
			&buffer.iter_at_offset(found.start),
			&buffer.iter_at_offset(found.end),
		);
		imp.current_match.set(Some(index));
		if reveal {
			self.reveal(found);
		}
	}

	/// The scrolling position and the column widths set by the user, to
	/// restore them after reloading the document.
	pub fn view_state(&self) -> ViewState {
		let scroll = self
			.vadjustment()
			.map_or(0.0, |adjustment| adjustment.value());
		let table_widths = self
			.imp()
			.embedded
			.borrow()
			.iter()
			.filter_map(|embedded| embedded.widget.downcast_ref::<Table>())
			.map(Table::user_widths)
			.collect();
		ViewState {
			scroll,
			table_widths,
		}
	}

	/// Restores a state saved by [`Self::view_state`], as far as the
	/// document still matches it.
	pub fn restore_view_state(&self, state: &ViewState) {
		let embedded = self.imp().embedded.borrow();
		let tables = embedded
			.iter()
			.filter_map(|embedded| embedded.widget.downcast_ref::<Table>());
		for (table, widths) in tables.zip(&state.table_widths) {
			if let Some(widths) = widths {
				table.set_user_widths(widths);
			}
		}

		drop(embedded);
		self.scroll_to_position(state.scroll);
	}

	/// The vertical scrolling position, in pixels.
	pub fn scroll_position(&self) -> f64 {
		self.vadjustment()
			.map_or(0.0, |adjustment| adjustment.value())
	}

	/// Scrolls to a position returned by [`Self::scroll_position`].
	pub fn scroll_to_position(&self, scroll: f64) {
		// The layout is validated progressively: scroll at each frame until
		// the position is reached and stays there.
		let frames = Cell::new(0);
		let reached = Cell::new(0);
		self.add_tick_callback(move |view, _| {
			frames.set(frames.get() + 1);
			let Some(adjustment) = view.vadjustment() else {
				return glib::ControlFlow::Break;
			};
			let value = scroll.min(adjustment.upper() - adjustment.page_size());
			if (adjustment.value() - value).abs() < 1.0 {
				reached.set(reached.get() + 1);
			} else {
				reached.set(0);
				adjustment.set_value(value);
			}
			if reached.get() >= 2 || frames.get() > MAX_RESTORE_FRAMES {
				glib::ControlFlow::Break
			} else {
				glib::ControlFlow::Continue
			}
		});
	}

	/// Scrolls so that a match is visible, even inside an embedded widget.
	fn reveal(&self, found: &Match) {
		// Marks can be scrolled to before the layout is complete.
		let buffer = self.buffer();
		let offset = found.anchor.unwrap_or(found.start);
		let mark = buffer
			.mark(REVEAL_MARK)
			.unwrap_or_else(|| buffer.create_mark(Some(REVEAL_MARK), &buffer.start_iter(), true));
		buffer.move_mark(&mark, &buffer.iter_at_offset(offset));
		self.scroll_to_mark(&mark, 0.2, false, 0.0, 0.0);

		if found.anchor.is_some() {
			// The layout is validated progressively, and the widget may be
			// taller than the view: adjust the scrolling at each frame until the
			// match is visible.
			let found = found.clone();
			let frames = Cell::new(0);
			self.add_tick_callback(move |view, _| {
				frames.set(frames.get() + 1);
				if view.reveal_in_embedded(&found) || frames.get() > MAX_REVEAL_FRAMES {
					glib::ControlFlow::Break
				} else {
					glib::ControlFlow::Continue
				}
			});
		}
	}

	/// Scrolls towards a match inside an embedded widget. Returns whether it
	/// is visible.
	fn reveal_in_embedded(&self, found: &Match) -> bool {
		let Some(adjustment) = self.vadjustment() else {
			return true;
		};
		if !found.view.is_mapped() {
			return false;
		}
		let mut start = found.view.buffer().iter_at_offset(found.start);
		// Scroll inside the widget too (code blocks scroll horizontally).
		found.view.scroll_to_iter(&mut start, 0.0, false, 0.0, 0.0);
		let location = found.view.iter_location(&start);
		let (x, y) = found.view.buffer_to_window_coords(
			gtk::TextWindowType::Widget,
			location.x(),
			location.y(),
		);
		let Some(point) = found
			.view
			.compute_point(self, &graphene::Point::new(x as f32, y as f32))
		else {
			return false;
		};
		let page = adjustment.page_size();
		let top = f64::from(point.y());
		let bottom = top + f64::from(location.height());
		if top >= 0.0 && bottom <= page {
			return true;
		}
		let value = adjustment.value() + top - page * 0.2;
		adjustment.set_value(value.clamp(adjustment.lower(), adjustment.upper() - page));
		false
	}

	/// The vertical position of the heading with the given anchor, in the
	/// coordinates of [`Self::scroll_position`].
	pub fn anchor_position(&self, anchor: &str) -> Option<f64> {
		let anchors = self.imp().anchors.borrow();
		let mark = anchors.get(anchor)?;
		let (y, _) = self.line_yrange(&self.buffer().iter_at_mark(mark));
		Some(f64::from(y))
	}

	/// Scrolls to the heading with the given anchor. Returns whether it exists.
	pub fn scroll_to_anchor(&self, anchor: &str) -> bool {
		let anchors = self.imp().anchors.borrow();
		match anchors.get(anchor) {
			Some(mark) => {
				self.scroll_to_mark(mark, 0.0, true, 0.0, 0.0);
				true
			}
			None => false,
		}
	}

	pub fn connect_link_activated<F: Fn(&Self, &str) + 'static>(
		&self,
		callback: F,
	) -> glib::SignalHandlerId {
		self.connect_closure(
			"link-activated",
			false,
			glib::closure_local!(move |view: &Self, url: String| callback(view, &url)),
		)
	}

	fn setup(&self) {
		self.add_css_class("markdown-view");
		self.set_editable(false);
		self.set_cursor_visible(false);
		self.set_wrap_mode(gtk::WrapMode::WordChar);
		self.set_top_margin(24);
		self.set_bottom_margin(24);
		self.set_left_margin(24);
		self.set_right_margin(24);
		self.set_has_tooltip(true);

		self.create_tags();
		self.update_colors();

		let style_manager = adw::StyleManager::default();
		let handlers = vec![
			style_manager.connect_dark_notify(glib::clone!(
				#[weak(rename_to = view)]
				self,
				move |_| view.update_colors()
			)),
			style_manager.connect_accent_color_notify(glib::clone!(
				#[weak(rename_to = view)]
				self,
				move |_| view.update_colors()
			)),
		];
		self.imp().style_handlers.replace(handlers);

		let click = gtk::GestureClick::new();
		click.set_button(gdk::BUTTON_PRIMARY);
		click.connect_released(glib::clone!(
			#[weak(rename_to = view)]
			self,
			move |_, n_press, x, y| {
				if n_press != 1 || view.buffer().has_selection() {
					return;
				}
				if let Some(url) = view.link_at(x, y) {
					view.emit_by_name::<()>("link-activated", &[&url]);
				}
			}
		));
		self.add_controller(click);

		let motion = gtk::EventControllerMotion::new();
		motion.connect_motion(glib::clone!(
			#[weak(rename_to = view)]
			self,
			move |_, x, y| {
				let cursor = if view.link_at(x, y).is_some() {
					"pointer"
				} else {
					"text"
				};
				view.set_cursor_from_name(Some(cursor));
			}
		));
		self.add_controller(motion);

		self.connect_query_tooltip(|view, x, y, keyboard, tooltip| {
			if keyboard {
				return false;
			}
			match view.link_at(f64::from(x), f64::from(y)) {
				Some(url) => {
					tooltip.set_text(Some(&url));
					true
				}
				None => false,
			}
		});
	}

	/// The URL of the link under the given widget coordinates.
	fn link_at(&self, x: f64, y: f64) -> Option<String> {
		let (x, y) = self.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
		let (iter, _) = self.iter_at_position(x, y)?;
		// `iter_at_position` returns the closest character: make sure the
		// pointer is really over it.
		let location = self.iter_location(&iter);
		if x < location.x() || x >= location.x() + location.width().max(1) {
			return None;
		}
		let offset = iter.offset();
		self.imp()
			.links
			.borrow()
			.iter()
			.find(|link| link.start <= offset && offset < link.end)
			.map(|link| link.url.clone())
	}

	fn create_tags(&self) {
		let table = self.buffer().tag_table();
		let add = |tag: gtk::TextTag| {
			table.add(&tag);
		};

		for (index, (name, scale)) in HEADING_TAGS.iter().zip(HEADING_SCALES).enumerate() {
			add(gtk::TextTag::builder()
				.name(*name)
				.scale(scale)
				.weight(if index < 2 { 800 } else { 700 })
				.pixels_above_lines(if index < 2 { 20 } else { 14 })
				.pixels_below_lines(8)
				.build());
		}
		for spacing in SPACINGS {
			add(gtk::TextTag::builder()
				.name(spacing_tag_name(spacing))
				.pixels_below_lines(spacing)
				.build());
		}
		add(gtk::TextTag::builder().name("strong").weight(700).build());
		add(gtk::TextTag::builder()
			.name("superscript")
			.scale(0.75)
			.rise(5 * pango::SCALE)
			.build());
		for kind in ALERT_KINDS {
			add(gtk::TextTag::builder()
				.name(alert_tag_name(kind))
				.weight(700)
				.build());
		}
		add(gtk::TextTag::builder()
			.name("emphasis")
			.style(pango::Style::Italic)
			.build());
		add(gtk::TextTag::builder()
			.name("strikethrough")
			.strikethrough(true)
			.build());
		add(gtk::TextTag::builder()
			.name("code")
			.family("monospace")
			.build());
		add(gtk::TextTag::builder()
			.name("link")
			.underline(pango::Underline::Single)
			.build());
	}

	/// Uses the monospace font of the preferences for inline code.
	fn update_code_font(&self) {
		let family = appearance::monospace_font().family();
		if let Some(tag) = self.buffer().tag_table().lookup("code") {
			tag.set_family(Some(family.as_deref().unwrap_or("monospace")));
		}
	}

	/// Width available for text, excluding the view margins.
	fn text_width(&self) -> i32 {
		self.width() - self.left_margin() - self.right_margin()
	}

	/// Makes embedded widgets as wide as the text.
	fn resize_embedded(&self) {
		let text_width = self.text_width();
		if text_width <= 0 {
			return;
		}
		for embedded in self.imp().embedded.borrow().iter() {
			let width = (text_width - embedded.indent).max(1);
			// Images keep their own size, within the available width.
			if let Some(image) = embedded.widget.downcast_ref::<Image>() {
				image.fit(width);
			} else if embedded.widget.width_request() != width {
				embedded.widget.set_size_request(width, -1);
				if let Some(table) = embedded.widget.downcast_ref::<Table>() {
					table.fit(width);
				}
			}
		}
	}

	/// The selected text, with embedded widgets replaced by their text.
	fn selected_text(&self) -> Option<String> {
		let buffer = self.buffer();
		let (start, end) = buffer.selection_bounds()?;
		let embedded = self.imp().embedded.borrow();
		let mut text = String::new();
		for (offset, character) in (start.offset()..).zip(buffer.slice(&start, &end, true).chars())
		{
			if character != '\u{fffc}' {
				text.push(character);
				continue;
			}
			if let Some(embedded) = embedded.iter().find(|embedded| embedded.offset == offset) {
				text.push_str(&embedded.text);
			}
		}
		Some(text)
	}

	/// Returns the tag with the given name, creating it with `build` if needed.
	fn tag(&self, name: &str, build: impl FnOnce(gtk::TextTag)) -> gtk::TextTag {
		let table = self.buffer().tag_table();
		table.lookup(name).unwrap_or_else(|| {
			let tag = gtk::TextTag::new(Some(name));
			build(tag.clone());
			table.add(&tag);
			tag
		})
	}

	fn draw_decorations(&self, snapshot: &gtk::Snapshot) {
		let decorations = self.imp().decorations.borrow();
		if decorations.is_empty() {
			return;
		}
		let buffer = self.buffer();
		let dark = adw::StyleManager::default().is_dark();
		let foreground = self.color();
		let color = gdk::RGBA::new(
			foreground.red(),
			foreground.green(),
			foreground.blue(),
			foreground.alpha() * 0.25,
		);
		let text_width = self.text_width();

		for decoration in decorations.iter() {
			match *decoration {
				Decoration::QuoteBar {
					indent,
					spacing,
					start,
					end,
					alert,
				} => {
					let color = alert.map_or(color, |kind| alert_color(kind, dark));
					let (top, _) = self.line_yrange(&buffer.iter_at_offset(start));
					let (last_top, last_height) = self.line_yrange(&buffer.iter_at_offset(end - 1));
					let bottom = last_top + last_height - spacing;
					let x = self.left_margin() + indent;
					snapshot.append_color(
						&color,
						&graphene::Rect::new(
							x as f32,
							top as f32,
							QUOTE_BAR_WIDTH,
							(bottom - top).max(0) as f32,
						),
					);
				}
				Decoration::Rule {
					indent,
					spacing,
					offset,
				} => {
					let (top, height) = self.line_yrange(&buffer.iter_at_offset(offset));
					let y = top as f32 + (height - spacing) as f32 / 2.0;
					let x = self.left_margin() + indent;
					snapshot.append_color(
						&color,
						&graphene::Rect::new(
							x as f32,
							y,
							(text_width - indent).max(0) as f32,
							RULE_HEIGHT,
						),
					);
				}
			}
		}
	}

	/// Updates the tag colors for the current light/dark style and accent color.
	fn update_colors(&self) {
		let style_manager = adw::StyleManager::default();
		let dark = style_manager.is_dark();
		let table = self.buffer().tag_table();
		let tag = |name: &str| table.lookup(name).expect("unknown text tag");

		let code_background = gdk::RGBA::new(0.5, 0.5, 0.5, 0.15);
		tag("code").set_background_rgba(Some(&code_background));
		for kind in ALERT_KINDS {
			tag(alert_tag_name(kind)).set_foreground_rgba(Some(&alert_color(kind, dark)));
		}
		tag("link")
			.set_foreground_rgba(Some(&style_manager.accent_color().to_standalone_rgba(dark)));
	}
}

/// Layout of the blocks being rendered.
#[derive(Debug, Clone, Copy)]
struct Context {
	/// Left indentation, in pixels.
	indent: i32,
	/// Space after each block, in pixels.
	spacing: i32,
	/// Nesting level of bullet lists.
	bullet_depth: usize,
}

impl Default for Context {
	fn default() -> Self {
		Self {
			indent: 0,
			spacing: BLOCK_SPACING,
			bullet_depth: 0,
		}
	}
}

const ALERT_KINDS: [AlertKind; 5] = [
	AlertKind::Note,
	AlertKind::Tip,
	AlertKind::Important,
	AlertKind::Warning,
	AlertKind::Caution,
];

fn alert_tag_name(kind: AlertKind) -> &'static str {
	match kind {
		AlertKind::Note => "alert-note",
		AlertKind::Tip => "alert-tip",
		AlertKind::Important => "alert-important",
		AlertKind::Warning => "alert-warning",
		AlertKind::Caution => "alert-caution",
	}
}

/// The default title of an alert.
fn alert_name(kind: AlertKind) -> String {
	match kind {
		AlertKind::Note => gettext("Note"),
		AlertKind::Tip => gettext("Tip"),
		AlertKind::Important => gettext("Important"),
		AlertKind::Warning => gettext("Warning"),
		AlertKind::Caution => gettext("Caution"),
	}
}

/// Color of an alert, from the GNOME palette.
fn alert_color(kind: AlertKind, dark: bool) -> gdk::RGBA {
	let (light_color, dark_color): ((u8, u8, u8), (u8, u8, u8)) = match kind {
		AlertKind::Note => ((0x1c, 0x71, 0xd8), (0x62, 0xa0, 0xea)),
		AlertKind::Tip => ((0x26, 0xa2, 0x69), (0x57, 0xe3, 0x89)),
		AlertKind::Important => ((0x91, 0x41, 0xac), (0xc0, 0x61, 0xcb)),
		AlertKind::Warning => ((0xc6, 0x46, 0x00), (0xff, 0xa3, 0x48)),
		AlertKind::Caution => ((0xc0, 0x1c, 0x28), (0xf6, 0x61, 0x51)),
	};
	let (red, green, blue) = if dark { dark_color } else { light_color };
	gdk::RGBA::new(
		f32::from(red) / 255.0,
		f32::from(green) / 255.0,
		f32::from(blue) / 255.0,
		1.0,
	)
}

/// Finds the occurrences of `query` in the buffer of `view`.
fn find_in_view(
	view: &gtk::TextView,
	query: &str,
	options: SearchOptions,
	anchor: Option<i32>,
) -> Vec<Match> {
	let buffer = view.buffer();
	let (start, end) = buffer.bounds();
	// The slice has one character per buffer offset, child anchors included.
	let text: Vec<char> = buffer.slice(&start, &end, true).chars().collect();
	search::find_all(&text, query, options)
		.into_iter()
		.map(|(start, end)| Match {
			view: view.clone(),
			start: start as i32,
			end: end as i32,
			anchor,
		})
		.collect()
}

/// The text views of an embedded widget, in reading order.
fn embedded_text_views(widget: &gtk::Widget) -> Vec<gtk::TextView> {
	if let Some(code_block) = widget.downcast_ref::<CodeBlock>() {
		code_block.text_view().into_iter().collect()
	} else if let Some(table) = widget.downcast_ref::<Table>() {
		table
			.cell_views()
			.into_iter()
			.map(|cell| cell.upcast())
			.collect()
	} else {
		Vec::new()
	}
}

/// Creates the search highlighting tags of a buffer if needed, with the
/// highest priorities.
fn ensure_search_tags(buffer: &gtk::TextBuffer) {
	let table = buffer.tag_table();
	if table.lookup(MATCH_TAG).is_some() {
		return;
	}
	table.add(
		&gtk::TextTag::builder()
			.name(MATCH_TAG)
			.background_rgba(&gdk::RGBA::new(0.96, 0.83, 0.18, 0.45))
			.build(),
	);
	table.add(
		&gtk::TextTag::builder()
			.name(CURRENT_MATCH_TAG)
			.background_rgba(&gdk::RGBA::new(1.0, 0.64, 0.28, 1.0))
			.foreground_rgba(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.9))
			.build(),
	);
}

struct Renderer<'a> {
	view: &'a MarkdownView,
	base_dir: Option<&'a gio::File>,
	buffer: &'a gtk::TextBuffer,
	links: Vec<Link>,
	anchors: HashMap<String, gtk::TextMark>,
	decorations: Vec<Decoration>,
	embedded: Vec<Embedded>,
}

impl Renderer<'_> {
	fn blocks(&mut self, blocks: &[Block], context: Context) {
		for block in blocks {
			self.block(block, context);
		}
	}

	fn block(&mut self, block: &Block, context: Context) {
		let start = self.offset();
		match block {
			Block::Heading {
				level,
				anchor,
				content,
			} => {
				let mark = self.buffer.create_mark(None, &self.buffer.end_iter(), true);
				self.anchors.insert(anchor.clone(), mark);
				let tag = HEADING_TAGS[usize::from(level.clamp(&1, &6) - 1)];
				self.spans(content, &[tag], context.indent);
				self.end_line();
				// Headings have their own spacing.
				self.indent(start, context.indent);
			}
			Block::Paragraph(content) => {
				self.spans(content, &[], context.indent);
				self.end_leaf_block(start, context);
			}
			Block::Code { info, text } => {
				let widget = CodeBlock::new(text, info);
				self.embed(widget.upcast(), text.clone(), context.indent);
				self.end_leaf_block(start, context);
			}
			Block::Table(table) => {
				let widget = Table::new(table, self.base_dir, self.view.link_callback());
				self.embed(widget.upcast(), table.to_tsv(), context.indent);
				self.end_leaf_block(start, context);
			}
			Block::Quote(blocks) => {
				let inner = Context {
					indent: context.indent + QUOTE_INDENT,
					..context
				};
				self.blocks(blocks, inner);
				if self.offset() > start {
					self.decorations.push(Decoration::QuoteBar {
						indent: context.indent,
						spacing: context.spacing,
						start,
						end: self.offset(),
						alert: None,
					});
					self.set_last_line_spacing(context.spacing);
				}
			}
			Block::Alert {
				kind,
				title,
				blocks,
			} => {
				let inner = Context {
					indent: context.indent + QUOTE_INDENT,
					..context
				};
				let title = title.clone().unwrap_or_else(|| alert_name(*kind));
				self.insert(&title, &[alert_tag_name(*kind)]);
				self.end_leaf_block(start, inner);
				self.blocks(blocks, inner);
				self.decorations.push(Decoration::QuoteBar {
					indent: context.indent,
					spacing: context.spacing,
					start,
					end: self.offset(),
					alert: Some(*kind),
				});
				self.set_last_line_spacing(context.spacing);
			}
			Block::Footnotes(footnotes) => self.footnotes(footnotes, context),
			Block::List(list) => self.list(list, context),
			Block::ThematicBreak => {
				self.end_leaf_block(start, context);
				self.decorations.push(Decoration::Rule {
					indent: context.indent,
					spacing: context.spacing,
					offset: start,
				});
			}
		}
	}

	fn list(&mut self, list: &List, context: Context) {
		let inner = Context {
			indent: context.indent + LIST_INDENT,
			spacing: if list.tight {
				TIGHT_SPACING
			} else {
				BLOCK_SPACING
			},
			bullet_depth: context.bullet_depth + usize::from(list.kind == ListKind::Bullet),
		};
		for (index, item) in list.items.iter().enumerate() {
			let marker = match (item.task, list.kind) {
				(Some(true), _) => "☑".to_owned(),
				(Some(false), _) => "☐".to_owned(),
				(None, ListKind::Bullet) => {
					BULLETS[context.bullet_depth % BULLETS.len()].to_owned()
				}
				(None, ListKind::Ordered { start, delimiter }) => {
					format!("{}{delimiter}", start + index)
				}
			};
			self.item(&marker, &item.blocks, inner);
		}
		self.set_last_line_spacing(context.spacing);
	}

	/// Renders a list item, with `marker` hanging in its indentation.
	fn item(&mut self, marker: &str, blocks: &[Block], inner: Context) {
		let marker = format!("{marker} ");
		let start = self.offset();
		self.insert(&marker, &[]);
		if !matches!(
			blocks.first(),
			Some(Block::Paragraph(_) | Block::Heading { .. })
		) {
			// The marker gets a line of its own.
			self.end_leaf_block(start, inner);
		}
		self.blocks(blocks, inner);

		// Hang the marker in the indentation of the item's first line. Pango
		// indents the following lines of a paragraph when the indent is
		// negative: shift the whole paragraph left, and indent it back.
		let (marker_width, _) = self.view.create_pango_layout(Some(&marker)).pixel_size();
		let margin = self.view.left_margin() + inner.indent - marker_width;
		let hang = self
			.view
			.tag(&format!("hang-{margin}-{marker_width}"), |tag| {
				tag.set_left_margin(margin);
				tag.set_indent(-marker_width);
			});
		let first_line_start = self.buffer.iter_at_offset(start);
		let mut first_line_end = first_line_start;
		first_line_end.forward_line();
		self.buffer.remove_tag(
			&self.indent_tag(inner.indent),
			&first_line_start,
			&first_line_end,
		);
		self.buffer
			.apply_tag(&hang, &first_line_start, &first_line_end);
	}

	/// Renders the footnote definitions, numbered like their references.
	fn footnotes(&mut self, footnotes: &[Footnote], context: Context) {
		let start = self.offset();
		self.end_leaf_block(start, context);
		self.decorations.push(Decoration::Rule {
			indent: context.indent,
			spacing: context.spacing,
			offset: start,
		});
		let inner = Context {
			indent: context.indent + LIST_INDENT,
			spacing: TIGHT_SPACING,
			..context
		};
		for (index, footnote) in footnotes.iter().enumerate() {
			let mark = self.buffer.create_mark(None, &self.buffer.end_iter(), true);
			self.anchors.insert(footnote.anchor.clone(), mark);
			self.item(&format!("{}.", index + 1), &footnote.blocks, inner);
		}
		self.set_last_line_spacing(context.spacing);
	}

	/// Embeds a widget at the end of the buffer.
	fn embed(&mut self, widget: gtk::Widget, text: String, indent: i32) {
		let offset = self.offset();
		let anchor = self.buffer.create_child_anchor(&mut self.buffer.end_iter());
		self.view.add_child_at_anchor(&widget, &anchor);
		self.embedded.push(Embedded {
			offset,
			indent,
			widget,
			text,
		});
	}

	fn spans(&mut self, spans: &[Span], block_tags: &[&str], indent: i32) {
		for span in spans {
			let mut tags = block_tags.to_vec();
			let style = span.style;
			if let Some(image) = &span.image {
				if let Some(file) = uri::local_file(&image.url, self.base_dir) {
					let widget = Image::new(
						&file,
						image,
						&span.text,
						span.link.as_deref(),
						self.view.link_callback(),
					);
					self.embed(widget.upcast(), span.text.clone(), indent);
					continue;
				}
				// Remote images are not loaded: show the alternative text.
				tags.push("emphasis");
			}
			if style.strong {
				tags.push("strong");
			}
			if style.emphasis {
				tags.push("emphasis");
			}
			if style.strikethrough {
				tags.push("strikethrough");
			}
			if style.code {
				tags.push("code");
			}
			if style.superscript {
				tags.push("superscript");
			}
			if span.link.is_some() {
				tags.push("link");
			}
			let start = self.offset();
			self.insert(&span.text, &tags);
			if let Some(url) = &span.link {
				self.links.push(Link {
					start,
					end: self.offset(),
					url: url.clone(),
				});
			}
		}
	}

	fn offset(&self) -> i32 {
		self.buffer.end_iter().offset()
	}

	fn insert(&self, text: &str, tags: &[&str]) {
		self.buffer
			.insert_with_tags_by_name(&mut self.buffer.end_iter(), text, tags);
	}

	fn end_line(&self) {
		self.insert("\n", &[]);
	}

	/// Ends a block that contains no other block.
	fn end_leaf_block(&self, start: i32, context: Context) {
		self.end_line();
		self.indent(start, context.indent);
		self.set_last_line_spacing(context.spacing);
	}

	/// Indents the lines from `start` to the end of the buffer.
	fn indent(&self, start: i32, indent: i32) {
		if indent == 0 {
			return;
		}
		self.buffer.apply_tag(
			&self.indent_tag(indent),
			&self.buffer.iter_at_offset(start),
			&self.buffer.end_iter(),
		);
	}

	/// Tag setting the left margin for an indentation relative to the view margin.
	///
	/// The left margin of a tag replaces the margin of the view.
	fn indent_tag(&self, indent: i32) -> gtk::TextTag {
		let margin = self.view.left_margin() + indent;
		self.view.tag(&format!("margin-{margin}"), |tag| {
			tag.set_left_margin(margin);
		})
	}

	/// Sets the space below the last line of the buffer.
	///
	/// Only the last line of a block gets spacing, so that hard line breaks
	/// inside the block are not spaced out.
	fn set_last_line_spacing(&self, spacing: i32) {
		let end = self.buffer.end_iter();
		let mut line_start = end;
		if !line_start.backward_line() {
			return;
		}
		for value in SPACINGS {
			self.buffer
				.remove_tag_by_name(&spacing_tag_name(value), &line_start, &end);
		}
		let tag = self.view.tag(&spacing_tag_name(spacing), |tag| {
			tag.set_pixels_below_lines(spacing);
		});
		self.buffer.apply_tag(&tag, &line_start, &end);
	}

	/// Removes the newline and the spacing ending the last block.
	fn finish(&self) {
		let mut end = self.buffer.end_iter();
		let mut last = end;
		if last.backward_char() && last.char() == '\n' {
			self.buffer.delete(&mut last, &mut end);
		}
		let end = self.buffer.end_iter();
		let mut line_start = end;
		line_start.set_line_offset(0);
		for value in SPACINGS {
			self.buffer
				.remove_tag_by_name(&spacing_tag_name(value), &line_start, &end);
		}
	}
}

fn spacing_tag_name(spacing: i32) -> String {
	format!("spacing-{spacing}")
}
