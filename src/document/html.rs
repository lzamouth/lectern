//! Raw HTML found in Markdown documents is not rendered: only its text and
//! its images are kept.

/// Elements whose content is never shown.
const HIDDEN_ELEMENTS: [&str; 4] = ["script", "style", "template", "title"];

/// Elements that start a new line.
const LINE_ELEMENTS: [&str; 20] = [
	"address",
	"blockquote",
	"br",
	"dd",
	"details",
	"div",
	"dl",
	"dt",
	"figcaption",
	"figure",
	"h1",
	"h2",
	"h3",
	"h4",
	"h5",
	"h6",
	"li",
	"p",
	"summary",
	"tr",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
	Text(String),
	Image(HtmlImage),
}

/// An `<img>` element.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HtmlImage {
	pub src: String,
	pub alt: String,
	pub title: String,
	pub width: Option<i32>,
	pub height: Option<i32>,
}

/// Extracts the text and images of an HTML fragment, by line.
///
/// Whitespace is collapsed and empty lines are dropped.
pub fn lines(html: &str) -> Vec<Vec<Inline>> {
	let mut lines = Vec::new();
	let mut line = Vec::new();
	let mut text = String::new();
	let mut rest = html;
	while let Some(start) = rest.find('<') {
		text.push_str(&rest[..start]);
		rest = &rest[start..];

		if let Some(comment) = rest.strip_prefix("<!--") {
			rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
			continue;
		}
		let Some(end) = rest.find('>') else {
			break;
		};
		let tag = &rest[..=end];
		rest = &rest[end + 1..];

		let (name, closing) = tag_name(tag);
		if !closing && HIDDEN_ELEMENTS.contains(&name.as_str()) {
			let close = format!("</{name}");
			rest = find_ignore_case(rest, &close)
				.and_then(|pos| rest[pos..].find('>').map(|end| &rest[pos + end + 1..]))
				.unwrap_or("");
		} else if LINE_ELEMENTS.contains(&name.as_str()) {
			flush_text(&mut text, &mut line);
			lines.push(std::mem::take(&mut line));
		} else if name == "img"
			&& let Some(image) = image(tag)
		{
			flush_text(&mut text, &mut line);
			line.push(Inline::Image(image));
		}
	}
	text.push_str(rest);
	flush_text(&mut text, &mut line);
	lines.push(line);

	lines.into_iter().filter_map(normalize_line).collect()
}

/// Parses an `<img>` tag. Returns `None` for other tags or without `src`.
pub fn image(tag: &str) -> Option<HtmlImage> {
	if tag_name(tag) != ("img".to_owned(), false) {
		return None;
	}
	let mut image = HtmlImage::default();
	for (name, value) in attributes(tag) {
		match name.as_str() {
			"src" => image.src = value,
			"alt" => image.alt = decode_entities(&value),
			"title" => image.title = decode_entities(&value),
			"width" => image.width = pixels(&value),
			"height" => image.height = pixels(&value),
			_ => {}
		}
	}
	(!image.src.is_empty()).then_some(image)
}

/// Lowercase name of a tag, and whether it is a closing tag.
fn tag_name(tag: &str) -> (String, bool) {
	let inner = tag.trim_start_matches('<');
	let closing = inner.starts_with('/');
	let name = inner
		.trim_start_matches('/')
		.split(|c: char| c.is_whitespace() || c == '/' || c == '>')
		.next()
		.unwrap_or_default()
		.to_ascii_lowercase();
	(name, closing)
}

/// Attributes of a tag, with lowercase names.
fn attributes(tag: &str) -> Vec<(String, String)> {
	let inner = tag
		.trim_start_matches('<')
		.trim_end_matches('>')
		.trim_end_matches('/');
	// Skip the tag name.
	let mut rest = inner.trim_start_matches(|c: char| !c.is_whitespace());
	let mut attributes = Vec::new();
	loop {
		rest = rest.trim_start();
		let name_end = rest
			.find(|c: char| c.is_whitespace() || c == '=')
			.unwrap_or(rest.len());
		if name_end == 0 {
			break;
		}
		let name = rest[..name_end].to_ascii_lowercase();
		rest = rest[name_end..].trim_start();
		let value = match rest.strip_prefix('=') {
			Some(after) => {
				let after = after.trim_start();
				let (value, remaining) = match after.chars().next() {
					Some(quote @ ('"' | '\'')) => {
						let end = after[1..].find(quote).map_or(after.len(), |end| end + 1);
						(&after[1..end], after.get(end + 1..).unwrap_or(""))
					}
					_ => {
						let end = after.find(char::is_whitespace).unwrap_or(after.len());
						(&after[..end], &after[end..])
					}
				};
				rest = remaining;
				value.to_owned()
			}
			None => String::new(),
		};
		attributes.push((name, value));
	}
	attributes
}

/// Parses a size attribute in pixels, ignoring percentages.
fn pixels(value: &str) -> Option<i32> {
	let value = value.trim().trim_end_matches("px");
	value
		.parse::<f64>()
		.ok()
		.map(|pixels| pixels.round() as i32)
		.filter(|pixels| *pixels > 0)
}

fn flush_text(text: &mut String, line: &mut Vec<Inline>) {
	if !text.is_empty() {
		line.push(Inline::Text(decode_entities(text)));
		text.clear();
	}
}

/// Collapses whitespace in a line, returning `None` if nothing is left.
fn normalize_line(line: Vec<Inline>) -> Option<Vec<Inline>> {
	let mut normalized: Vec<Inline> = Vec::new();
	for inline in line {
		match inline {
			Inline::Text(text) => {
				let mut collapsed = String::with_capacity(text.len());
				let mut previous_space = match normalized.last() {
					None => true,
					Some(Inline::Text(text)) => text.ends_with(' '),
					Some(Inline::Image(_)) => false,
				};
				for character in text.chars() {
					if character.is_whitespace() && character != '\u{a0}' {
						if !previous_space {
							collapsed.push(' ');
						}
						previous_space = true;
					} else {
						collapsed.push(character);
						previous_space = false;
					}
				}
				if !collapsed.is_empty() {
					normalized.push(Inline::Text(collapsed));
				}
			}
			image => normalized.push(image),
		}
	}
	if let Some(Inline::Text(text)) = normalized.last_mut() {
		let trimmed = text.trim_end_matches(' ').len();
		text.truncate(trimmed);
		if text.is_empty() {
			normalized.pop();
		}
	}
	(!normalized.is_empty()).then_some(normalized)
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
	haystack.to_ascii_lowercase().find(needle)
}

/// Decodes the most common character references.
fn decode_entities(text: &str) -> String {
	let mut decoded = String::with_capacity(text.len());
	let mut rest = text;
	while let Some(start) = rest.find('&') {
		decoded.push_str(&rest[..start]);
		rest = &rest[start..];
		let entity = rest[1..]
			.find(';')
			.filter(|&end| end <= 10)
			.map(|end| &rest[1..end + 1]);
		let character = entity.and_then(|entity| match entity {
			"amp" => Some('&'),
			"lt" => Some('<'),
			"gt" => Some('>'),
			"quot" => Some('"'),
			"apos" => Some('\''),
			"nbsp" => Some('\u{a0}'),
			_ => {
				let number = entity.strip_prefix('#')?;
				let code = match number.strip_prefix(['x', 'X']) {
					Some(hex) => u32::from_str_radix(hex, 16).ok()?,
					None => number.parse().ok()?,
				};
				char::from_u32(code)
			}
		});
		match (entity, character) {
			(Some(entity), Some(character)) => {
				decoded.push(character);
				rest = &rest[entity.len() + 2..];
			}
			_ => {
				decoded.push('&');
				rest = &rest[1..];
			}
		}
	}
	decoded.push_str(rest);
	decoded
}

#[cfg(test)]
mod tests {
	use super::*;

	fn text_lines(html: &str) -> Vec<String> {
		lines(html)
			.into_iter()
			.map(|line| {
				line.into_iter()
					.map(|inline| match inline {
						Inline::Text(text) => text,
						Inline::Image(image) => format!("[{}]", image.src),
					})
					.collect()
			})
			.collect()
	}

	#[test]
	fn keeps_text_and_images() {
		let html = "<p align=\"center\">\n  <img src=\"logo.png\">\n  <b>My</b> project\n</p>";
		assert_eq!(text_lines(html), ["[logo.png] My project"]);
	}

	#[test]
	fn hides_comments_and_scripts() {
		let html = "<!-- note -->\n<script>alert(1)</script><STYLE>p {}</style>visible";
		assert_eq!(text_lines(html), ["visible"]);
	}

	#[test]
	fn splits_lines_on_block_elements() {
		let html = "<details><summary>Title</summary>Body<br>end</details>";
		assert_eq!(text_lines(html), ["Title", "Body", "end"]);
	}

	#[test]
	fn decodes_entities() {
		assert_eq!(
			text_lines("a &amp; b &lt;&#65;&#x42;&gt; &bogus; &"),
			["a & b <AB> &bogus; &"]
		);
	}

	#[test]
	fn parses_image_attributes() {
		let image = image("<IMG width=\"120px\" height=50% alt='A &amp; B' src=x.png/>");
		assert_eq!(
			image,
			Some(HtmlImage {
				src: "x.png".to_owned(),
				alt: "A & B".to_owned(),
				title: String::new(),
				width: Some(120),
				height: None,
			})
		);
		assert_eq!(super::image("<img alt=\"no source\">"), None);
		assert_eq!(super::image("<b>"), None);
	}
}
