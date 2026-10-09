//! Color scheme and fonts chosen in the preferences.

use adw::prelude::*;
use gtk::glib::translate::IntoGlib;
use gtk::{gdk, gio, glib, pango};

use crate::config;

/// Keys of the settings affecting the rendering of documents.
pub const RENDERING_KEYS: [&str; 4] = [
	"margin",
	"use-system-fonts",
	"document-font",
	"monospace-font",
];

thread_local! {
	static SETTINGS: gio::Settings = gio::Settings::new(config::APP_ID);
	static CSS_PROVIDER: gtk::CssProvider = gtk::CssProvider::new();
}

pub fn settings() -> gio::Settings {
	SETTINGS.with(Clone::clone)
}

/// Applies the color scheme and fonts, and follows their changes.
pub fn init() {
	let settings = settings();
	let style_manager = adw::StyleManager::default();

	apply_color_scheme();
	settings.connect_changed(Some("color-scheme"), |_, _| apply_color_scheme());

	if let Some(display) = gdk::Display::default() {
		CSS_PROVIDER.with(|provider| {
			gtk::style_context_add_provider_for_display(
				&display,
				provider,
				gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
			);
		});
	}
	update_css();
	for key in ["use-system-fonts", "document-font", "monospace-font"] {
		settings.connect_changed(Some(key), |_, _| update_css());
	}
	style_manager.connect_document_font_name_notify(|_| update_css());
	style_manager.connect_monospace_font_name_notify(|_| update_css());
}

/// Calls `callback` when a setting affecting the rendering of documents changes.
pub fn connect_rendering_changed<F: Fn() + Clone + 'static>(
	callback: F,
) -> Vec<glib::SignalHandlerId> {
	let settings = settings();
	let mut handlers: Vec<glib::SignalHandlerId> = RENDERING_KEYS
		.iter()
		.map(|key| {
			let callback = callback.clone();
			settings.connect_changed(Some(key), move |_, _| callback())
		})
		.collect();
	let style_manager = adw::StyleManager::default();
	let document_callback = callback.clone();
	handlers.push(style_manager.connect_document_font_name_notify(move |_| document_callback()));
	handlers.push(style_manager.connect_monospace_font_name_notify(move |_| callback()));
	handlers
}

/// Disconnects handlers returned by [`connect_rendering_changed`].
pub fn disconnect_rendering_changed(handlers: Vec<glib::SignalHandlerId>) {
	let settings = settings();
	let style_manager = adw::StyleManager::default();
	let settings_count = RENDERING_KEYS.len();
	for (index, handler) in handlers.into_iter().enumerate() {
		if index < settings_count {
			settings.disconnect(handler);
		} else {
			style_manager.disconnect(handler);
		}
	}
}

/// Margin around the text of documents, in pixels.
pub fn margin() -> i32 {
	settings().int("margin")
}

/// Font of the text of documents.
pub fn document_font() -> pango::FontDescription {
	font(
		"document-font",
		adw::StyleManager::default().document_font_name(),
	)
}

/// Font of the code in documents.
pub fn monospace_font() -> pango::FontDescription {
	font(
		"monospace-font",
		adw::StyleManager::default().monospace_font_name(),
	)
}

fn font(key: &str, system_font: glib::GString) -> pango::FontDescription {
	let settings = settings();
	let name = if settings.boolean("use-system-fonts") {
		system_font
	} else {
		settings.string(key)
	};
	pango::FontDescription::from_string(&name)
}

fn apply_color_scheme() {
	let scheme = match settings().string("color-scheme").as_str() {
		"light" => adw::ColorScheme::ForceLight,
		"dark" => adw::ColorScheme::ForceDark,
		_ => adw::ColorScheme::Default,
	};
	adw::StyleManager::default().set_color_scheme(scheme);
}

fn update_css() {
	let css = format!(
		"textview.markdown-view {{ {} }}\n.code-block textview {{ {} }}\n",
		font_css(&document_font()),
		font_css(&monospace_font()),
	);
	CSS_PROVIDER.with(|provider| provider.load_from_string(&css));
}

/// CSS declarations for a Pango font description.
fn font_css(font: &pango::FontDescription) -> String {
	let mut css = String::new();
	if let Some(family) = font.family() {
		let family = family.replace('\\', "\\\\").replace('"', "\\\"");
		css.push_str(&format!("font-family: \"{family}\"; "));
	}
	if font.size() > 0 {
		let size = f64::from(font.size()) / f64::from(pango::SCALE);
		let unit = if font.is_size_absolute() { "px" } else { "pt" };
		css.push_str(&format!("font-size: {size}{unit}; "));
	}
	let weight: i32 = font.weight().into_glib();
	css.push_str(&format!("font-weight: {weight}; "));
	let style = match font.style() {
		pango::Style::Italic => "italic",
		pango::Style::Oblique => "oblique",
		_ => "normal",
	};
	css.push_str(&format!("font-style: {style};"));
	css
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn font_descriptions_to_css() {
		let font = pango::FontDescription::from_string("Noto \"Serif\" Bold Italic 12");
		assert_eq!(
			font_css(&font),
			"font-family: \"Noto \\\"Serif\\\"\"; font-size: 12pt; font-weight: 700; font-style: italic;"
		);
	}
}
