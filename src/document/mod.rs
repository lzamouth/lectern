//! Toolkit-independent model of a Markdown document, built from the comrak AST.

mod html;
mod parse;

pub use parse::parse;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Document {
	pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
	Heading {
		level: u8,
		/// GitHub-compatible anchor, used by `#fragment` links.
		anchor: String,
		content: Vec<Span>,
	},
	Paragraph(Vec<Span>),
	Code {
		info: String,
		text: String,
	},
	Quote(Vec<Block>),
	List(List),
	Table(Table),
	ThematicBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
	pub alignments: Vec<Alignment>,
	pub header: Vec<Cell>,
	pub rows: Vec<Vec<Cell>>,
}

impl Table {
	/// Tab-separated text of the table, one row per line.
	pub fn to_tsv(&self) -> String {
		std::iter::once(&self.header)
			.chain(&self.rows)
			.map(|row| {
				row.iter()
					.map(|cell| plain_text(cell).replace(['\t', '\n'], " "))
					.collect::<Vec<_>>()
					.join("\t")
			})
			.collect::<Vec<_>>()
			.join("\n")
	}
}

/// Content of a table cell.
pub type Cell = Vec<Span>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
	#[default]
	Default,
	Left,
	Center,
	Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
	pub kind: ListKind,
	/// Tight lists have no blank lines between their items.
	pub tight: bool,
	pub items: Vec<ListItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
	Bullet,
	Ordered { start: usize, delimiter: char },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
	/// `Some(checked)` for task list items.
	pub task: Option<bool>,
	pub blocks: Vec<Block>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Style {
	pub strong: bool,
	pub emphasis: bool,
	pub strikethrough: bool,
	pub code: bool,
}

/// A run of text sharing the same style and link target, or an image.
///
/// A `'\n'` in `text` is a hard line break inside the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
	/// The text, or the alternative text of an image.
	pub text: String,
	pub style: Style,
	pub link: Option<String>,
	pub image: Option<Image>,
}

impl Span {
	pub fn plain(text: &str) -> Self {
		Self {
			text: text.to_owned(),
			style: Style::default(),
			link: None,
			image: None,
		}
	}
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Image {
	/// URL or path, possibly relative to the document.
	pub url: String,
	pub title: String,
	/// Size requested by HTML attributes, in pixels.
	pub width: Option<i32>,
	pub height: Option<i32>,
}

/// Concatenated text of spans.
pub fn plain_text(spans: &[Span]) -> String {
	spans.iter().map(|span| span.text.as_str()).collect()
}
