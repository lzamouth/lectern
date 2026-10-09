//! Raw HTML found in Markdown documents is not rendered: only its text is kept.

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

/// Extracts the text of an HTML fragment, one entry per non-empty line.
pub fn text_lines(html: &str) -> Vec<String> {
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
		let tag = &rest[1..end];
		rest = &rest[end + 1..];

		let closing = tag.starts_with('/');
		let name = tag
			.trim_start_matches('/')
			.split(|c: char| c.is_whitespace() || c == '/')
			.next()
			.unwrap_or_default()
			.to_ascii_lowercase();
		if !closing && HIDDEN_ELEMENTS.contains(&name.as_str()) {
			let close = format!("</{name}");
			rest = find_ignore_case(rest, &close)
				.and_then(|pos| rest[pos..].find('>').map(|end| &rest[pos + end + 1..]))
				.unwrap_or("");
		} else if LINE_ELEMENTS.contains(&name.as_str()) {
			text.push('\n');
		}
	}
	text.push_str(rest);

	decode_entities(&text)
		.lines()
		.map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
		.filter(|line| !line.is_empty())
		.collect()
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

	#[test]
	fn keeps_text_only() {
		let html = "<p align=\"center\">\n  <img src=\"logo.png\">\n  <b>My</b> project\n</p>";
		assert_eq!(text_lines(html), ["My project"]);
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
}
