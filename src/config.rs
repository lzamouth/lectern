//! Build-time configuration, provided by Meson through environment variables.
//! Defaults allow plain `cargo` builds (for `cargo check`, rust-analyzer, etc.).

pub const APP_ID: &str = "com.skunkcrafts.Lectern";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GETTEXT_PACKAGE: &str = "lectern";

const LOCALEDIR: &str = match option_env!("LECTERN_CONFIG_LOCALEDIR") {
	Some(dir) => dir,
	None => "/usr/local/share/locale",
};

/// Directory holding the compiled translations.
///
/// `LECTERN_LOCALEDIR` overrides it at runtime; `meson devenv` sets it so that
/// translations work from the build directory.
pub fn localedir() -> String {
	std::env::var("LECTERN_LOCALEDIR").unwrap_or_else(|_| LOCALEDIR.to_owned())
}
