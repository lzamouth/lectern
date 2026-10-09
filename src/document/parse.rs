use comrak::nodes::{AstNode, NodeValue};
use comrak::{Anchorizer, Arena, Options};

use super::{Block, Document, Span, Style, plain_text};

pub fn parse(markdown: &str) -> Document {
	let mut options = Options::default();
	options.extension.strikethrough = true;
	options.extension.table = true;
	options.extension.autolink = true;
	options.extension.tasklist = true;
	options.extension.front_matter_delimiter = Some("---".to_owned());

	let arena = Arena::new();
	let root = comrak::parse_document(&arena, markdown, &options);

	let mut builder = Builder::default();
	builder.blocks(root);
	Document {
		blocks: builder.blocks,
	}
}

#[derive(Default)]
struct Builder {
	blocks: Vec<Block>,
	anchorizer: Anchorizer,
}

impl Builder {
	fn blocks<'a>(&mut self, node: &'a AstNode<'a>) {
		for child in node.children() {
			self.block(child);
		}
	}

	fn block<'a>(&mut self, node: &'a AstNode<'a>) {
		match &node.data.borrow().value {
			NodeValue::FrontMatter(_) => {}
			NodeValue::Paragraph => {
				let content = inlines(node);
				if !content.is_empty() {
					self.blocks.push(Block::Paragraph(content));
				}
			}
			NodeValue::Heading(heading) => {
				let content = inlines(node);
				let anchor = self.anchorizer.anchorize(&plain_text(&content));
				self.blocks.push(Block::Heading {
					level: heading.level,
					anchor,
					content,
				});
			}
			NodeValue::CodeBlock(code) => {
				let text = code.literal.strip_suffix('\n').unwrap_or(&code.literal);
				self.blocks.push(Block::Code {
					info: code.info.clone(),
					text: text.to_owned(),
				});
			}
			NodeValue::HtmlBlock(html) => {
				let text = html.literal.trim_end();
				if !text.is_empty() {
					self.blocks.push(Block::Html(text.to_owned()));
				}
			}
			NodeValue::ThematicBreak => self.blocks.push(Block::ThematicBreak),
			// Provisional rendering until lists get their own blocks.
			NodeValue::Item(_) => self.prefixed_item(node, "•\u{a0}"),
			NodeValue::TaskItem(task) => {
				let marker = if task.symbol.is_some() {
					"☑\u{a0}"
				} else {
					"☐\u{a0}"
				};
				self.prefixed_item(node, marker);
			}
			// Provisional rendering until tables get their own widget.
			NodeValue::TableRow(header) => {
				let mut content = Vec::new();
				for (index, cell) in node.children().enumerate() {
					if index > 0 {
						content.push(Span::plain(" │ "));
					}
					let style = Style {
						strong: *header,
						..Style::default()
					};
					collect_inlines(cell, style, None, &mut content);
				}
				self.blocks.push(Block::Paragraph(content));
			}
			_ => self.blocks(node),
		}
	}

	fn prefixed_item<'a>(&mut self, node: &'a AstNode<'a>, marker: &str) {
		let first = self.blocks.len();
		self.blocks(node);
		match self.blocks.get_mut(first) {
			Some(Block::Paragraph(content)) => content.insert(0, Span::plain(marker)),
			_ => self
				.blocks
				.insert(first, Block::Paragraph(vec![Span::plain(marker)])),
		}
	}
}

fn inlines<'a>(node: &'a AstNode<'a>) -> Vec<Span> {
	let mut spans = Vec::new();
	collect_inlines(node, Style::default(), None, &mut spans);
	// Hard breaks at the end of a block are meaningless.
	while let Some(last) = spans.last_mut() {
		let trimmed = last.text.trim_end_matches('\n').len();
		last.text.truncate(trimmed);
		if !last.text.is_empty() {
			break;
		}
		spans.pop();
	}
	spans
}

fn collect_inlines<'a>(
	node: &'a AstNode<'a>,
	style: Style,
	link: Option<&str>,
	spans: &mut Vec<Span>,
) {
	for child in node.children() {
		match &child.data.borrow().value {
			NodeValue::Text(text) => push_text(spans, text, style, link),
			NodeValue::SoftBreak => push_text(spans, " ", style, link),
			NodeValue::LineBreak => push_text(spans, "\n", style, link),
			NodeValue::Code(code) => {
				let style = Style {
					code: true,
					..style
				};
				push_text(spans, &code.literal, style, link);
			}
			NodeValue::Emph => {
				let style = Style {
					emphasis: true,
					..style
				};
				collect_inlines(child, style, link, spans);
			}
			NodeValue::Strong => {
				let style = Style {
					strong: true,
					..style
				};
				collect_inlines(child, style, link, spans);
			}
			NodeValue::Strikethrough => {
				let style = Style {
					strikethrough: true,
					..style
				};
				collect_inlines(child, style, link, spans);
			}
			NodeValue::Link(target) => collect_inlines(child, style, Some(&target.url), spans),
			// Provisional rendering until images are supported: show the alternative text.
			NodeValue::Image(_) => {
				let style = Style {
					emphasis: true,
					..style
				};
				collect_inlines(child, style, link, spans);
			}
			NodeValue::HtmlInline(html) => {
				if is_html_line_break(html) {
					push_text(spans, "\n", style, link);
				}
			}
			_ => collect_inlines(child, style, link, spans),
		}
	}
}

fn push_text(spans: &mut Vec<Span>, text: &str, style: Style, link: Option<&str>) {
	if let Some(last) = spans.last_mut()
		&& last.style == style
		&& last.link.as_deref() == link
	{
		last.text.push_str(text);
		return;
	}
	spans.push(Span {
		text: text.to_owned(),
		style,
		link: link.map(str::to_owned),
	});
}

fn is_html_line_break(html: &str) -> bool {
	let tag: String = html
		.chars()
		.filter(|c| !c.is_whitespace())
		.collect::<String>()
		.to_ascii_lowercase();
	matches!(tag.as_str(), "<br>" | "<br/>")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn spans(markdown: &str) -> Vec<Span> {
		match parse(markdown).blocks.into_iter().next() {
			Some(Block::Paragraph(spans)) => spans,
			other => panic!("expected a paragraph, got {other:?}"),
		}
	}

	#[test]
	fn headings_get_github_anchors() {
		let document = parse("# Hello World\n\n## Hello World\n");
		let anchors: Vec<_> = document
			.blocks
			.iter()
			.map(|block| match block {
				Block::Heading { level, anchor, .. } => (*level, anchor.as_str()),
				other => panic!("unexpected block {other:?}"),
			})
			.collect();
		assert_eq!(anchors, [(1, "hello-world"), (2, "hello-world-1")]);
	}

	#[test]
	fn emphasis_is_nested() {
		let spans = spans("a **b *c*** ~~d~~ `e`");
		let styles: Vec<_> = spans
			.iter()
			.map(|span| (span.text.as_str(), span.style))
			.collect();
		let strong = Style {
			strong: true,
			..Style::default()
		};
		assert_eq!(
			styles,
			[
				("a ", Style::default()),
				("b ", strong),
				(
					"c",
					Style {
						emphasis: true,
						..strong
					}
				),
				(" ", Style::default()),
				(
					"d",
					Style {
						strikethrough: true,
						..Style::default()
					}
				),
				(" ", Style::default()),
				(
					"e",
					Style {
						code: true,
						..Style::default()
					}
				),
			]
		);
	}

	#[test]
	fn links_and_autolinks_keep_their_target() {
		let spans = spans("[a](b.md) https://example.org");
		assert_eq!(spans[0].link.as_deref(), Some("b.md"));
		assert_eq!(spans[2].text, "https://example.org");
		assert_eq!(spans[2].link.as_deref(), Some("https://example.org"));
	}

	#[test]
	fn breaks() {
		assert_eq!(plain_text(&spans("a\nb")), "a b");
		assert_eq!(plain_text(&spans("a  \nb<br>c\\")), "a\nb\nc\\");
	}

	#[test]
	fn front_matter_is_hidden() {
		let document = parse("---\ntitle: x\n---\n\ntext\n");
		assert_eq!(
			document.blocks,
			[Block::Paragraph(vec![Span::plain("text")])]
		);
	}

	#[test]
	fn code_block_drops_final_newline() {
		let document = parse("```rust\nfn main() {}\n```\n");
		assert_eq!(
			document.blocks,
			[Block::Code {
				info: "rust".to_owned(),
				text: "fn main() {}".to_owned(),
			}]
		);
	}
}
