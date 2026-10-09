//! Toolkit-independent model of a Markdown document, built from the comrak AST.

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
	/// Raw HTML, which is not interpreted.
	Html(String),
	ThematicBreak,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Style {
	pub strong: bool,
	pub emphasis: bool,
	pub strikethrough: bool,
	pub code: bool,
}

/// A run of text sharing the same style and link target.
///
/// A `'\n'` in `text` is a hard line break inside the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
	pub text: String,
	pub style: Style,
	pub link: Option<String>,
}

impl Span {
	pub fn plain(text: &str) -> Self {
		Self {
			text: text.to_owned(),
			style: Style::default(),
			link: None,
		}
	}
}

/// Concatenated text of spans.
pub fn plain_text(spans: &[Span]) -> String {
	spans.iter().map(|span| span.text.as_str()).collect()
}
