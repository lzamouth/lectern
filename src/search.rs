//! Text search, independent of the toolkit.

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
	pub case_sensitive: bool,
	/// Matches must not be part of a longer word.
	pub whole_words: bool,
}

/// Finds the non-overlapping occurrences of `needle` in `haystack`.
///
/// Returns their ranges as character offsets, end excluded.
pub fn find_all(haystack: &[char], needle: &str, options: SearchOptions) -> Vec<(usize, usize)> {
	let needle: Vec<char> = needle.chars().collect();
	let mut matches = Vec::new();
	if needle.is_empty() || needle.len() > haystack.len() {
		return matches;
	}
	let needs_start_boundary = options.whole_words && is_word_char(needle[0]);
	let needs_end_boundary = options.whole_words && is_word_char(needle[needle.len() - 1]);

	let mut start = 0;
	while start + needle.len() <= haystack.len() {
		let end = start + needle.len();
		let found = haystack[start..end]
			.iter()
			.zip(&needle)
			.all(|(a, b)| chars_match(*a, *b, options.case_sensitive))
			&& !(needs_start_boundary && start > 0 && is_word_char(haystack[start - 1]))
			&& !(needs_end_boundary && end < haystack.len() && is_word_char(haystack[end]));
		if found {
			matches.push((start, end));
			start = end;
		} else {
			start += 1;
		}
	}
	matches
}

fn chars_match(a: char, b: char, case_sensitive: bool) -> bool {
	a == b || (!case_sensitive && a.to_lowercase().eq(b.to_lowercase()))
}

fn is_word_char(c: char) -> bool {
	c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
	use super::*;

	fn find(
		haystack: &str,
		needle: &str,
		case_sensitive: bool,
		whole_words: bool,
	) -> Vec<(usize, usize)> {
		let haystack: Vec<char> = haystack.chars().collect();
		find_all(
			&haystack,
			needle,
			SearchOptions {
				case_sensitive,
				whole_words,
			},
		)
	}

	#[test]
	fn case() {
		assert_eq!(find("Été, été", "été", false, false), [(0, 3), (5, 8)]);
		assert_eq!(find("Été, été", "été", true, false), [(5, 8)]);
	}

	#[test]
	fn whole_words() {
		assert_eq!(
			find("cat concat cat_x cat.", "cat", false, true),
			[(0, 3), (17, 20)]
		);
		assert_eq!(find("a cat", "cat", false, false), [(2, 5)]);
		// Boundaries only matter next to word characters of the needle.
		assert_eq!(find("f(x) gf(x)", "(x)", false, true), [(1, 4), (7, 10)]);
	}

	#[test]
	fn non_overlapping() {
		assert_eq!(find("aaaa", "aa", false, false), [(0, 2), (2, 4)]);
	}

	#[test]
	fn empty_or_too_long() {
		assert!(find("abc", "", false, false).is_empty());
		assert!(find("ab", "abc", false, false).is_empty());
	}
}
