cargo = find_program('cargo')

# flatpak-builder builds with debugoptimized.
if get_option('buildtype') in ['release', 'minsize', 'debugoptimized']
	cargo_profile = 'release'
else
	cargo_profile = 'debug'
endif

cargo_env = environment()
cargo_env.set('LECTERN_CONFIG_LOCALEDIR', localedir)

custom_target(
	'cargo-build',
	build_by_default: true,
	build_always_stale: true,
	console: true,
	output: meson.project_name(),
	env: cargo_env,
	install: true,
	install_dir: bindir,
	command: [
		meson.project_source_root() / 'build-aux' / 'cargo-build.sh',
		cargo,
		meson.project_source_root() / 'Cargo.toml',
		meson.project_build_root() / 'target',
		cargo_profile,
		'@OUTPUT@',
	],
)

test(
	'cargo-test',
	cargo,
	args: [
		'test',
		'--manifest-path', meson.project_source_root() / 'Cargo.toml',
		'--target-dir', meson.project_build_root() / 'target',
	],
	env: cargo_env,
	timeout: 600,
)
