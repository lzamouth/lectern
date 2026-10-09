mod application;
mod config;
#[cfg(debug_assertions)]
mod devtools;
mod document;
mod search;
mod uri;
mod widgets;
mod window;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::application::LecternApplication;

fn main() -> glib::ExitCode {
	// SAFETY: called first thing in main, before any other thread exists.
	unsafe {
		setlocale(LocaleCategory::LcAll, "");
	}
	bindtextdomain(config::GETTEXT_PACKAGE, config::localedir())
		.expect("Unable to bind the text domain");
	bind_textdomain_codeset(config::GETTEXT_PACKAGE, "UTF-8")
		.expect("Unable to set the text domain encoding");
	textdomain(config::GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

	gio::resources_register_include!("lectern.gresource")
		.expect("Unable to register the resources");

	glib::set_application_name("Lectern");

	LecternApplication::new().run()
}
