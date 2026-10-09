//! A table with columns resizable with the mouse.
//!
//! Columns are separated by gaps holding a vertical line: dragging a gap
//! resizes the column on its left, double-clicking it fits the column to its
//! content.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib, graphene, gsk, pango};

use super::MarkdownView;
use crate::document::{self, Alignment, Span, Style};

/// Width of the gaps between columns, holding the column lines.
const GAP: i32 = 7;
const LINE_WIDTH: f32 = 1.0;
/// Horizontal and vertical padding of the cell text.
const CELL_PADDING_X: i32 = 6;
const CELL_PADDING_Y: i32 = 6;
const MIN_COLUMN_WIDTH: i32 = 40;
/// Columns are not made narrower than this to fit the window: the table
/// scrolls horizontally instead.
const MIN_FITTED_COLUMN_WIDTH: i32 = 120;
const MAX_NATURAL_COLUMN_WIDTH: i32 = 600;

mod imp {
	use super::*;

	#[derive(Debug, Default)]
	pub struct Table {
		pub(super) grid: RefCell<Option<super::TableGrid>>,
		pub(super) tsv: RefCell<String>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for Table {
		const NAME: &'static str = "LecternTable";
		type Type = super::Table;
		type ParentType = adw::Bin;
	}

	impl ObjectImpl for Table {}
	impl WidgetImpl for Table {}
	impl BinImpl for Table {}

	#[derive(Debug, Default)]
	pub struct TableGrid {
		/// Cells by row, the header row first.
		pub(super) cells: RefCell<Vec<Vec<MarkdownView>>>,
		pub(super) widths: RefCell<Vec<i32>>,
		/// Widths fitting the content of each column.
		pub(super) natural_widths: RefCell<Vec<i32>>,
		/// Set once the user resized a column: widths are no longer fitted.
		pub(super) user_resized: Cell<bool>,
		/// Height of each row, from the last allocation.
		pub(super) row_heights: RefCell<Vec<i32>>,
		/// Column being resized, and its width when the drag started.
		pub(super) drag: Cell<Option<(usize, i32)>>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for TableGrid {
		const NAME: &'static str = "LecternTableGrid";
		type Type = super::TableGrid;
		type ParentType = gtk::Widget;
	}

	impl ObjectImpl for TableGrid {
		fn constructed(&self) {
			self.parent_constructed();
			self.obj().setup_controllers();
		}

		fn dispose(&self) {
			for cell in self.cells.take().into_iter().flatten() {
				cell.unparent();
			}
		}
	}

	impl WidgetImpl for TableGrid {
		fn request_mode(&self) -> gtk::SizeRequestMode {
			gtk::SizeRequestMode::HeightForWidth
		}

		fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
			let obj = self.obj();
			let size = match orientation {
				gtk::Orientation::Horizontal => obj.total_width(),
				_ => {
					let heights = obj.compute_row_heights();
					heights.iter().sum::<i32>() + heights.len() as i32 + 1
				}
			};
			(size, size, -1, -1)
		}

		fn size_allocate(&self, _width: i32, _height: i32, _baseline: i32) {
			let obj = self.obj();
			let heights = obj.compute_row_heights();
			let widths = self.widths.borrow();
			let mut y = 1;
			for (row, height) in self.cells.borrow().iter().zip(&heights) {
				let mut x = GAP;
				for (cell, width) in row.iter().zip(widths.iter()) {
					let transform =
						gsk::Transform::new().translate(&graphene::Point::new(x as f32, y as f32));
					cell.allocate(*width, *height, -1, Some(transform));
					x += width + GAP;
				}
				y += height + 1;
			}
			self.row_heights.replace(heights);
		}

		fn snapshot(&self, snapshot: &gtk::Snapshot) {
			let obj = self.obj();
			obj.draw_grid(snapshot);
			for cell in self.cells.borrow().iter().flatten() {
				obj.snapshot_child(cell, snapshot);
			}
		}
	}
}

glib::wrapper! {
	/// The table, scrolling horizontally when wider than the space available.
	pub struct Table(ObjectSubclass<imp::Table>)
		@extends adw::Bin, gtk::Widget,
		@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

glib::wrapper! {
	/// Lays out the cells and draws the table lines.
	pub struct TableGrid(ObjectSubclass<imp::TableGrid>)
		@extends gtk::Widget,
		@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Table {
	/// Creates the table. `link_activated` is called when a link in a cell is clicked.
	pub fn new(
		table: &document::Table,
		base_dir: Option<&gio::File>,
		link_activated: impl Fn(&str) + Clone + 'static,
	) -> Self {
		let obj: Self = glib::Object::new();
		obj.add_css_class("markdown-table");

		let grid: TableGrid = glib::Object::new();
		grid.set_halign(gtk::Align::Start);
		grid.set_cells(table, base_dir, link_activated);

		let scrolled = gtk::ScrolledWindow::builder()
			.hscrollbar_policy(gtk::PolicyType::Automatic)
			.vscrollbar_policy(gtk::PolicyType::Never)
			.child(&grid)
			.build();
		obj.set_child(Some(&scrolled));

		let imp = obj.imp();
		imp.grid.replace(Some(grid));
		imp.tsv.replace(table.to_tsv());
		obj
	}

	/// Tab-separated text of the table.
	pub fn text(&self) -> String {
		self.imp().tsv.borrow().clone()
	}

	/// The views of the cells, row by row, for searching.
	pub fn cell_views(&self) -> Vec<MarkdownView> {
		self.imp()
			.grid
			.borrow()
			.as_ref()
			.map(|grid| {
				grid.imp()
					.cells
					.borrow()
					.iter()
					.flatten()
					.cloned()
					.collect()
			})
			.unwrap_or_default()
	}

	/// Fits the columns in `width` pixels, unless the user resized them.
	pub fn fit(&self, width: i32) {
		if let Some(grid) = self.imp().grid.borrow().as_ref() {
			grid.fit(width);
		}
	}
}

impl TableGrid {
	fn set_cells(
		&self,
		table: &document::Table,
		base_dir: Option<&gio::File>,
		link_activated: impl Fn(&str) + Clone + 'static,
	) {
		let column_count = table
			.rows
			.iter()
			.map(Vec::len)
			.chain([table.header.len(), table.alignments.len()])
			.max()
			.unwrap_or_default();

		let empty = Vec::new();
		let mut cells = Vec::new();
		let mut natural_widths = vec![MIN_COLUMN_WIDTH; column_count];
		for (row_index, row) in std::iter::once(&table.header)
			.chain(&table.rows)
			.enumerate()
		{
			let header = row_index == 0;
			let mut row_cells = Vec::new();
			for (column, natural_width) in natural_widths.iter_mut().enumerate() {
				let content = row.get(column).unwrap_or(&empty);
				let content: Vec<Span> = if header {
					content
						.iter()
						.map(|span| Span {
							style: Style {
								strong: true,
								..span.style
							},
							..span.clone()
						})
						.collect()
				} else {
					content.clone()
				};
				let alignment = table.alignments.get(column).copied().unwrap_or_default();
				let cell = MarkdownView::new_cell(&content, justification(alignment), base_dir);
				cell.set_margins(CELL_PADDING_X, CELL_PADDING_Y);
				let callback = link_activated.clone();
				cell.connect_link_activated(move |_, url| callback(url));
				cell.set_parent(self);

				let width = text_width(&cell, &document::plain_text(&content), header);
				*natural_width = (*natural_width).max(width + 2 * CELL_PADDING_X);
				row_cells.push(cell);
			}
			cells.push(row_cells);
		}

		let imp = self.imp();
		imp.cells.replace(cells);
		imp.widths.replace(
			natural_widths
				.iter()
				.map(|width| (*width).min(MAX_NATURAL_COLUMN_WIDTH))
				.collect(),
		);
		imp.natural_widths.replace(natural_widths);
	}

	/// Fits the columns in `available` pixels, unless the user resized them.
	///
	/// Columns keep their natural width when the table fits; otherwise the
	/// widest columns are narrowed first, down to a minimum width.
	fn fit(&self, available: i32) {
		let imp = self.imp();
		if imp.user_resized.get() {
			return;
		}
		let natural: Vec<i32> = imp
			.natural_widths
			.borrow()
			.iter()
			.map(|width| (*width).min(MAX_NATURAL_COLUMN_WIDTH))
			.collect();
		let column_space = available - GAP * (natural.len() as i32 + 1);
		let widths = if natural.iter().sum::<i32>() <= column_space {
			natural
		} else {
			let cap = width_cap(&natural, column_space).max(MIN_FITTED_COLUMN_WIDTH);
			natural.iter().map(|width| (*width).min(cap)).collect()
		};
		if *imp.widths.borrow() != widths {
			imp.widths.replace(widths);
			self.queue_resize();
		}
	}

	fn total_width(&self) -> i32 {
		let widths = self.imp().widths.borrow();
		widths.iter().sum::<i32>() + GAP * (widths.len() as i32 + 1)
	}

	fn compute_row_heights(&self) -> Vec<i32> {
		let widths = self.imp().widths.borrow();
		self.imp()
			.cells
			.borrow()
			.iter()
			.map(|row| {
				row.iter()
					.zip(widths.iter())
					.map(|(cell, width)| cell.measure(gtk::Orientation::Vertical, *width).1)
					.max()
					.unwrap_or_default()
			})
			.collect()
	}

	/// The column whose right-hand gap contains `x`.
	fn column_at_gap(&self, x: f64) -> Option<usize> {
		let mut right = 0;
		for (column, width) in self.imp().widths.borrow().iter().enumerate() {
			right += GAP + width;
			if x >= f64::from(right) && x < f64::from(right + GAP) {
				return Some(column);
			}
		}
		None
	}

	fn setup_controllers(&self) {
		let motion = gtk::EventControllerMotion::new();
		motion.connect_motion(glib::clone!(
			#[weak(rename_to = grid)]
			self,
			move |_, x, _| {
				let cursor = grid.column_at_gap(x).map(|_| "col-resize");
				grid.set_cursor_from_name(cursor);
			}
		));
		self.add_controller(motion);

		let drag = gtk::GestureDrag::new();
		drag.connect_drag_begin(glib::clone!(
			#[weak(rename_to = grid)]
			self,
			move |gesture, x, _| match grid.column_at_gap(x) {
				Some(column) => {
					let width = grid.imp().widths.borrow()[column];
					grid.imp().drag.set(Some((column, width)));
					gesture.set_state(gtk::EventSequenceState::Claimed);
				}
				None => {
					gesture.set_state(gtk::EventSequenceState::Denied);
				}
			}
		));
		drag.connect_drag_update(glib::clone!(
			#[weak(rename_to = grid)]
			self,
			move |_, offset_x, _| {
				if let Some((column, start_width)) = grid.imp().drag.get() {
					let width = (start_width + offset_x as i32).max(MIN_COLUMN_WIDTH);
					grid.set_column_width(column, width);
				}
			}
		));
		drag.connect_drag_end(glib::clone!(
			#[weak(rename_to = grid)]
			self,
			move |_, _, _| grid.imp().drag.set(None)
		));
		self.add_controller(drag);

		let click = gtk::GestureClick::new();
		click.connect_pressed(glib::clone!(
			#[weak(rename_to = grid)]
			self,
			move |_, n_press, x, _| {
				if n_press == 2
					&& let Some(column) = grid.column_at_gap(x)
				{
					let width = grid.imp().natural_widths.borrow()[column];
					grid.set_column_width(column, width);
				}
			}
		));
		self.add_controller(click);
	}

	fn set_column_width(&self, column: usize, width: i32) {
		let imp = self.imp();
		imp.user_resized.set(true);
		let mut widths = imp.widths.borrow_mut();
		if widths[column] != width {
			widths[column] = width;
			drop(widths);
			self.queue_resize();
		}
	}

	fn draw_grid(&self, snapshot: &gtk::Snapshot) {
		let imp = self.imp();
		let heights = imp.row_heights.borrow();
		if heights.is_empty() {
			return;
		}
		let foreground = self.color();
		let line_color = gdk::RGBA::new(
			foreground.red(),
			foreground.green(),
			foreground.blue(),
			foreground.alpha() * 0.2,
		);
		let header_color = gdk::RGBA::new(
			foreground.red(),
			foreground.green(),
			foreground.blue(),
			foreground.alpha() * 0.05,
		);

		let left = (GAP / 2) as f32;
		let right = (self.total_width() - GAP / 2) as f32;
		let height = (heights.iter().sum::<i32>() + heights.len() as i32 + 1) as f32;

		snapshot.append_color(
			&header_color,
			&graphene::Rect::new(left, 0.0, right - left, (heights[0] + 1) as f32),
		);

		// Horizontal lines, above each row and below the last one.
		let mut y = 0;
		for row_height in heights.iter().chain([&0]) {
			snapshot.append_color(
				&line_color,
				&graphene::Rect::new(left, y as f32, right - left + LINE_WIDTH, LINE_WIDTH),
			);
			y += row_height + 1;
		}

		// Vertical lines, in the middle of each gap.
		let mut x = left;
		for width in imp.widths.borrow().iter().chain([&0]) {
			snapshot.append_color(
				&line_color,
				&graphene::Rect::new(x, 0.0, LINE_WIDTH, height),
			);
			x += (width + GAP) as f32;
		}
	}
}

/// The largest column width such that the columns, capped to it, fit in `space`.
fn width_cap(widths: &[i32], space: i32) -> i32 {
	let mut sorted = widths.to_vec();
	sorted.sort_unstable();
	let mut remaining = space;
	for (index, width) in sorted.iter().enumerate() {
		let columns_left = (sorted.len() - index) as i32;
		if width * columns_left > remaining {
			return remaining / columns_left;
		}
		remaining -= width;
	}
	sorted.last().copied().unwrap_or_default()
}

fn justification(alignment: Alignment) -> gtk::Justification {
	match alignment {
		Alignment::Default | Alignment::Left => gtk::Justification::Left,
		Alignment::Center => gtk::Justification::Center,
		Alignment::Right => gtk::Justification::Right,
	}
}

/// Width of `text` on a single line, in the font of `widget`.
fn text_width(widget: &impl IsA<gtk::Widget>, text: &str, bold: bool) -> i32 {
	let layout = widget.create_pango_layout(Some(text));
	if bold {
		let attributes = pango::AttrList::new();
		attributes.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
		layout.set_attributes(Some(&attributes));
	}
	layout.pixel_size().0 + 1
}

#[cfg(test)]
mod tests {
	use super::width_cap;

	#[test]
	fn cap_narrows_widest_columns_first() {
		assert_eq!(width_cap(&[100, 300, 500], 600), 250);
		assert_eq!(width_cap(&[100, 200], 1000), 200);
		assert_eq!(width_cap(&[300, 300], 300), 150);
	}
}
