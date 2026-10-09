//! A read-only text view that renders a [`Document`] and handles its links.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use adw::prelude::*;
use gtk::glib::subclass::Signal;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, pango};

use crate::document::{Block, Document, List, ListKind, Span};

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

/// Graphics drawn below the text. Offsets are buffer character offsets.
#[derive(Debug)]
enum Decoration {
	/// Vertical bar on the left of the lines in `start..end`.
	QuoteBar {
		indent: i32,
		spacing: i32,
		start: i32,
		end: i32,
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

	impl TextViewImpl for MarkdownView {
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
	pub fn set_document(&self, document: &Document) {
		let buffer = self.buffer();
		buffer.set_text("");
		let mut renderer = Renderer {
			view: self,
			buffer: &buffer,
			links: Vec::new(),
			anchors: HashMap::new(),
			decorations: Vec::new(),
		};
		renderer.blocks(&document.blocks, Context::default());
		renderer.finish();

		let imp = self.imp();
		imp.links.replace(renderer.links);
		imp.decorations.replace(renderer.decorations);
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
			.name("link")
			.underline(pango::Underline::Single)
			.build());
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
		let foreground = self.color();
		let color = gdk::RGBA::new(
			foreground.red(),
			foreground.green(),
			foreground.blue(),
			foreground.alpha() * 0.25,
		);
		let text_width = self.visible_rect().width() - self.left_margin() - self.right_margin();

		for decoration in decorations.iter() {
			match *decoration {
				Decoration::QuoteBar {
					indent,
					spacing,
					start,
					end,
				} => {
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
		tag("code-block").set_paragraph_background_rgba(Some(&code_background));
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

struct Renderer<'a> {
	view: &'a MarkdownView,
	buffer: &'a gtk::TextBuffer,
	links: Vec<Link>,
	anchors: HashMap<String, gtk::TextMark>,
	decorations: Vec<Decoration>,
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
				self.spans(content, &[tag]);
				self.end_line();
				// Headings have their own spacing.
				self.indent(start, context.indent);
			}
			Block::Paragraph(content) => {
				self.spans(content, &[]);
				self.end_leaf_block(start, context);
			}
			Block::Code { text, .. } => {
				self.insert(text, &["code-block"]);
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
					});
					self.set_last_line_spacing(context.spacing);
				}
			}
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
			let marker = format!("{marker} ");

			let start = self.offset();
			self.insert(&marker, &[]);
			if !matches!(
				item.blocks.first(),
				Some(Block::Paragraph(_) | Block::Heading { .. })
			) {
				// The marker gets a line of its own.
				self.end_leaf_block(start, inner);
			}
			self.blocks(&item.blocks, inner);

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
		self.set_last_line_spacing(context.spacing);
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

	/// Removes the newline ending the last block.
	fn finish(&self) {
		let mut end = self.buffer.end_iter();
		let mut last = end;
		if last.backward_char() && last.char() == '\n' {
			self.buffer.delete(&mut last, &mut end);
		}
	}
}

fn spacing_tag_name(spacing: i32) -> String {
	format!("spacing-{spacing}")
}
