//! A read-only, syntax-highlighted code block with a copy button.

use std::cell::RefCell;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use sourceview5::prelude::*;

use crate::appearance;

/// Info strings that GtkSourceView does not recognize by language id or file extension.
const LANGUAGE_ALIASES: [(&str, &str); 9] = [
	("bash", "sh"),
	("console", "sh"),
	("shell", "sh"),
	("zsh", "sh"),
	("python", "python3"),
	("javascript", "js"),
	("typescript", "typescript"),
	("c++", "cpp"),
	("yml", "yaml"),
];

mod imp {
	use super::*;

	#[derive(Debug, Default)]
	pub struct CodeBlock {
		pub(super) buffer: RefCell<Option<sourceview5::Buffer>>,
		pub(super) view: RefCell<Option<sourceview5::View>>,
		pub(super) copy_button: RefCell<Option<gtk::Button>>,
		pub(super) dark_handler: RefCell<Option<glib::SignalHandlerId>>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for CodeBlock {
		const NAME: &'static str = "LecternCodeBlock";
		type Type = super::CodeBlock;
		type ParentType = adw::Bin;
	}

	impl ObjectImpl for CodeBlock {
		fn dispose(&self) {
			if let Some(handler) = self.dark_handler.take() {
				adw::StyleManager::default().disconnect(handler);
			}
		}
	}

	impl WidgetImpl for CodeBlock {}
	impl BinImpl for CodeBlock {}
}

glib::wrapper! {
	pub struct CodeBlock(ObjectSubclass<imp::CodeBlock>)
		@extends adw::Bin, gtk::Widget,
		@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl CodeBlock {
	/// Creates a code block for `text`, highlighted according to the
	/// language named by the first word of the Markdown info string.
	pub fn new(text: &str, info: &str) -> Self {
		let block: Self = glib::Object::new();
		block.setup(text, info);
		block
	}

	fn setup(&self, text: &str, info: &str) {
		self.add_css_class("code-block");
		self.set_overflow(gtk::Overflow::Hidden);

		let buffer = sourceview5::Buffer::new(None);
		buffer.set_text(text);
		buffer.set_highlight_matching_brackets(false);
		buffer.set_language(language_for(info).as_ref());

		let view = sourceview5::View::with_buffer(&buffer);
		view.set_editable(false);
		view.set_cursor_visible(false);
		view.set_monospace(true);
		view.set_tab_width(4);
		view.set_top_margin(12);
		view.set_bottom_margin(12);
		view.set_left_margin(12);
		view.set_right_margin(12);

		let scrolled = gtk::ScrolledWindow::builder()
			.hscrollbar_policy(gtk::PolicyType::Automatic)
			.vscrollbar_policy(gtk::PolicyType::Never)
			.child(&view)
			.build();

		let copy_button = gtk::Button::builder()
			.icon_name("edit-copy-symbolic")
			.tooltip_text(gettext("Copy"))
			.halign(gtk::Align::End)
			.valign(gtk::Align::Start)
			.margin_top(6)
			.margin_end(6)
			.css_classes(["flat", "circular"])
			.build();
		copy_button.connect_clicked(glib::clone!(
			#[weak(rename_to = block)]
			self,
			move |_| block.copy()
		));

		let overlay = gtk::Overlay::builder().child(&scrolled).build();
		overlay.add_overlay(&copy_button);
		self.set_child(Some(&overlay));

		let imp = self.imp();
		imp.buffer.replace(Some(buffer));
		imp.view.replace(Some(view));
		imp.copy_button.replace(Some(copy_button));

		self.update_style_scheme();
		let handler = adw::StyleManager::default().connect_dark_notify(glib::clone!(
			#[weak(rename_to = block)]
			self,
			move |_| block.update_style_scheme()
		));
		imp.dark_handler.replace(Some(handler));
	}

	/// The code shown by the block.
	pub fn text(&self) -> String {
		self.imp()
			.buffer
			.borrow()
			.as_ref()
			.map(|buffer| {
				let (start, end) = buffer.bounds();
				buffer.text(&start, &end, true).to_string()
			})
			.unwrap_or_default()
	}

	/// The view showing the code, for searching.
	pub fn text_view(&self) -> Option<gtk::TextView> {
		self.imp().view.borrow().clone().map(|view| view.upcast())
	}

	fn copy(&self) {
		self.clipboard().set_text(&self.text());

		// Acknowledge the copy by briefly changing the button icon.
		let Some(button) = self.imp().copy_button.borrow().clone() else {
			return;
		};
		button.set_icon_name("object-select-symbolic");
		glib::timeout_add_local_once(
			Duration::from_millis(1500),
			glib::clone!(
				#[weak]
				button,
				move || button.set_icon_name("edit-copy-symbolic")
			),
		);
	}

	fn update_style_scheme(&self) {
		let scheme = appearance::code_style_scheme(adw::StyleManager::default().is_dark());
		if let Some(buffer) = self.imp().buffer.borrow().as_ref() {
			buffer.set_style_scheme(scheme.as_ref());
		}
	}
}

/// Finds the GtkSourceView language for a Markdown info string.
fn language_for(info: &str) -> Option<sourceview5::Language> {
	let name = info.split_whitespace().next()?.to_ascii_lowercase();
	let manager = sourceview5::LanguageManager::default();
	let id = LANGUAGE_ALIASES
		.iter()
		.find(|(alias, _)| *alias == name)
		.map_or(name.as_str(), |(_, id)| id);
	manager
		.language(id)
		.or_else(|| manager.guess_language(Some(format!("file.{name}")), None))
}
