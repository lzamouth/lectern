use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{glib, pango};

use crate::appearance;

mod imp {
	use super::*;

	#[derive(Debug, Default, gtk::CompositeTemplate)]
	#[template(resource = "/com/skunkcrafts/Lectern/ui/preferences_dialog.ui")]
	pub struct LecternPreferencesDialog {
		#[template_child]
		pub color_scheme_toggles: TemplateChild<adw::ToggleGroup>,
		#[template_child]
		pub code_scheme_row: TemplateChild<adw::ComboRow>,
		#[template_child]
		pub margin_row: TemplateChild<adw::SpinRow>,
		#[template_child]
		pub remote_images_row: TemplateChild<adw::SwitchRow>,
		#[template_child]
		pub system_fonts_row: TemplateChild<adw::SwitchRow>,
		#[template_child]
		pub document_font_button: TemplateChild<gtk::FontDialogButton>,
		#[template_child]
		pub monospace_font_button: TemplateChild<gtk::FontDialogButton>,
	}

	#[glib::object_subclass]
	impl ObjectSubclass for LecternPreferencesDialog {
		const NAME: &'static str = "LecternPreferencesDialog";
		type Type = super::LecternPreferencesDialog;
		type ParentType = adw::PreferencesDialog;

		fn class_init(klass: &mut Self::Class) {
			klass.bind_template();
		}

		fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
			obj.init_template();
		}
	}

	impl ObjectImpl for LecternPreferencesDialog {
		fn constructed(&self) {
			self.parent_constructed();
			self.obj().bind_settings();
		}
	}

	impl WidgetImpl for LecternPreferencesDialog {}
	impl AdwDialogImpl for LecternPreferencesDialog {}
	impl PreferencesDialogImpl for LecternPreferencesDialog {}
}

glib::wrapper! {
	pub struct LecternPreferencesDialog(ObjectSubclass<imp::LecternPreferencesDialog>)
		@extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
		@implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl LecternPreferencesDialog {
	pub fn new() -> Self {
		glib::Object::new()
	}

	fn bind_settings(&self) {
		let imp = self.imp();
		let settings = appearance::settings();

		settings
			.bind("color-scheme", &*imp.color_scheme_toggles, "active-name")
			.build();
		settings.bind("margin", &*imp.margin_row, "value").build();
		settings
			.bind("load-remote-images", &*imp.remote_images_row, "active")
			.build();
		settings
			.bind("use-system-fonts", &*imp.system_fonts_row, "active")
			.build();

		for (key, button) in [
			("document-font", &*imp.document_font_button),
			("monospace-font", &*imp.monospace_font_button),
		] {
			settings
				.bind(key, button, "font-desc")
				.mapping(|variant, _| {
					let name = variant.str()?;
					Some(pango::FontDescription::from_string(name).to_value())
				})
				.set_mapping(|value, _| {
					let font = value.get::<pango::FontDescription>().ok()?;
					Some(font.to_string().to_variant())
				})
				.build();
		}

		// Code color schemes, by name.
		let schemes = appearance::code_style_schemes();
		let names: Vec<&str> = schemes.iter().map(|(_, name)| name.as_str()).collect();
		imp.code_scheme_row
			.set_model(Some(&gtk::StringList::new(&names)));
		let current = settings.string("code-style-scheme");
		if let Some(index) = schemes.iter().position(|(id, _)| *id == current) {
			imp.code_scheme_row.set_selected(index as u32);
		}
		imp.code_scheme_row.connect_selected_notify(move |row| {
			if let Some((id, _)) = schemes.get(row.selected() as usize) {
				let _ = appearance::settings().set_string("code-style-scheme", id);
			}
		});

		// Only offer monospace fonts for code.
		let filter = gtk::CustomFilter::new(|item| {
			item.downcast_ref::<pango::FontFamily>()
				.is_some_and(|family| family.is_monospace())
				|| item
					.downcast_ref::<pango::FontFace>()
					.is_some_and(|face| face.family().is_monospace())
		});
		let dialog = gtk::FontDialog::builder()
			.title(gettext("Monospace Font"))
			.filter(&filter)
			.build();
		imp.monospace_font_button.set_dialog(&dialog);
	}
}

impl Default for LecternPreferencesDialog {
	fn default() -> Self {
		Self::new()
	}
}
