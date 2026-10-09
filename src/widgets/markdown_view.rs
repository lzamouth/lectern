//! A read-only text view that renders a [`Document`] and handles its links.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use adw::prelude::*;
use gtk::glib::subclass::Signal;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, pango};

use crate::document::{Block, Document, Span};

const HEADING_SCALES: [f64; 6] = [2.0, 1.6, 1.35, 1.17, 1.0, 0.9];
const HEADING_TAGS: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];

/// Text range of a link in the buffer, as character offsets.
#[derive(Debug)]
struct Link {
	start: i32,
	end: i32,
	url: String,
}

mod imp {
	use super::*;

	#[derive(Debug, Default)]
	pub struct MarkdownView {
		pub(super) links: RefCell<Vec<Link>>,
		pub(super) anchors: RefCell<HashMap<String, gtk::TextMark>>,
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

	impl WidgetImpl for MarkdownView {}
	impl TextViewImpl for MarkdownView {}
}

glib::wrapper! {
	pub struct MarkdownView(ObjectSubclass<imp::MarkdownView>)
		@extends gtk::TextView, gtk::Widget,
		@implements gtk::Accessible, gtk::AccessibleText, gtk::Buildable,
			gtk::ConstraintTarget, gtk::Scrollable;
}

impl MarkdownView {
	pub fn set_document(&self, document: &Document) {
		let buffer = self.buffer();
		buffer.set_text("");
		let mut renderer = Renderer {
			buffer: &buffer,
			links: Vec::new(),
			anchors: HashMap::new(),
		};
		for block in &document.blocks {
			renderer.block(block);
		}
		renderer.finish();

		let imp = self.imp();
		imp.links.replace(renderer.links);
		let old_anchors = imp.anchors.replace(renderer.anchors);
		for mark in old_anchors.values() {
			buffer.delete_mark(mark);
		}
		buffer.place_cursor(&buffer.start_iter());
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

		add(gtk::TextTag::builder()
			.name("paragraph")
			.pixels_below_lines(12)
			.build());
		for (index, (name, scale)) in HEADING_TAGS.iter().zip(HEADING_SCALES).enumerate() {
			add(gtk::TextTag::builder()
				.name(*name)
				.scale(scale)
				.weight(if index < 2 { 800 } else { 700 })
				.pixels_above_lines(if index < 2 { 20 } else { 14 })
				.pixels_below_lines(8)
				.build());
		}
		add(gtk::TextTag::builder().name("strong").weight(700).build());
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
			.name("code-block")
			.family("monospace")
			.wrap_mode(gtk::WrapMode::Char)
			.build());
		add(gtk::TextTag::builder()
			.name("html")
			.family("monospace")
			.build());
		add(gtk::TextTag::builder()
			.name("rule")
			.justification(gtk::Justification::Center)
			.build());
		add(gtk::TextTag::builder()
			.name("link")
			.underline(pango::Underline::Single)
			.build());
	}

	/// Updates the tag colors for the current light/dark style and accent color.
	fn update_colors(&self) {
		let style_manager = adw::StyleManager::default();
		let dark = style_manager.is_dark();
		let table = self.buffer().tag_table();
		let tag = |name: &str| table.lookup(name).expect("unknown text tag");

		let shade = if dark { 1.0 } else { 0.0 };
		let code_background = gdk::RGBA::new(0.5, 0.5, 0.5, 0.15);
		let dimmed = gdk::RGBA::new(shade, shade, shade, 0.55);

		tag("code").set_background_rgba(Some(&code_background));
		tag("code-block").set_paragraph_background_rgba(Some(&code_background));
		tag("html").set_foreground_rgba(Some(&dimmed));
		tag("rule").set_foreground_rgba(Some(&dimmed));
		tag("link")
			.set_foreground_rgba(Some(&style_manager.accent_color().to_standalone_rgba(dark)));
	}
}

struct Renderer<'a> {
	buffer: &'a gtk::TextBuffer,
	links: Vec<Link>,
	anchors: HashMap<String, gtk::TextMark>,
}

impl Renderer<'_> {
	fn block(&mut self, block: &Block) {
		match block {
			Block::Heading {
				level,
				anchor,
				content,
			} => {
				let mark = self.buffer.create_mark(None, &self.buffer.end_iter(), true);
				self.anchors.insert(anchor.clone(), mark);
				let tag = HEADING_TAGS[usize::from(level.clamp(&1, &6) - 1)];
				self.spans(content, &[tag]);
				self.end_block(tag);
			}
			Block::Paragraph(content) => {
				self.spans(content, &[]);
				self.end_block("paragraph");
			}
			Block::Code { text, .. } => {
				self.insert(text, &["code-block"]);
				self.end_block("paragraph");
			}
			Block::Html(html) => {
				self.insert(html, &["html"]);
				self.end_block("paragraph");
			}
			Block::ThematicBreak => {
				self.insert("⁂", &["rule"]);
				self.end_block("paragraph");
			}
		}
	}

	fn spans(&mut self, spans: &[Span], block_tags: &[&str]) {
		for span in spans {
			let mut tags = block_tags.to_vec();
			let style = span.style;
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
			if span.link.is_some() {
				tags.push("link");
			}
			let start = self.buffer.end_iter().offset();
			self.insert(&span.text, &tags);
			if let Some(url) = &span.link {
				self.links.push(Link {
					start,
					end: self.buffer.end_iter().offset(),
					url: url.clone(),
				});
			}
		}
	}

	fn insert(&self, text: &str, tags: &[&str]) {
		self.buffer
			.insert_with_tags_by_name(&mut self.buffer.end_iter(), text, tags);
	}

	/// Ends the current block, applying `spacing_tag` to its last line only so
	/// that hard line breaks inside the block are not spaced out.
	fn end_block(&self, spacing_tag: &str) {
		self.insert("\n", &[]);
		let end = self.buffer.end_iter();
		let mut line_start = end;
		line_start.backward_line();
		self.buffer
			.apply_tag_by_name(spacing_tag, &line_start, &end);
	}

	/// Removes the newline ending the last block.
	fn finish(&self) {
		let mut end = self.buffer.end_iter();
		let mut last = end;
		if last.backward_char() && last.char() == '\n' {
			self.buffer.delete(&mut last, &mut end);
		}
	}
}
