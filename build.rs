use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const UI_DIR: &str = "data/resources/ui";

fn main() {
	let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
	let ui_out = out_dir.join("ui");
	fs::create_dir_all(&ui_out).expect("Unable to create the UI output directory");

	println!("cargo::rerun-if-changed=data/resources");
	println!("cargo::rerun-if-changed=data/icons");

	// Compile Blueprint files to GtkBuilder XML.
	let blueprints: Vec<PathBuf> = fs::read_dir(UI_DIR)
		.expect("Unable to read the UI directory")
		.filter_map(|entry| entry.ok().map(|entry| entry.path()))
		.filter(|path| path.extension().is_some_and(|ext| ext == "blp"))
		.collect();
	let status = Command::new("blueprint-compiler")
		.arg("batch-compile")
		.arg(&ui_out)
		.arg(UI_DIR)
		.args(&blueprints)
		.status()
		.expect("Unable to run blueprint-compiler, is it installed?");
	assert!(status.success(), "blueprint-compiler failed");

	glib_build_tools::compile_resources(
		&[
			out_dir.as_path(),
			Path::new("data/resources"),
			Path::new("data/icons"),
		],
		"data/resources/resources.gresource.xml",
		"lectern.gresource",
	);
}
