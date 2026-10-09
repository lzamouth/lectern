desktop_file = i18n.merge_file(
	input: application_id + '.desktop.in',
	output: application_id + '.desktop',
	type: 'desktop',
	po_dir: '../po',
	install: true,
	install_dir: datadir / 'applications',
)

desktop_file_validate = find_program('desktop-file-validate', required: false)
if desktop_file_validate.found()
	test('validate-desktop', desktop_file_validate, args: [desktop_file])
endif

metainfo_file = i18n.merge_file(
	input: application_id + '.metainfo.xml.in',
	output: application_id + '.metainfo.xml',
	po_dir: '../po',
	install: true,
	install_dir: datadir / 'metainfo',
)

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
	test('validate-metainfo', appstreamcli, args: ['validate', '--no-net', '--explain', metainfo_file])
endif

install_data(
	application_id + '.gschema.xml',
	install_dir: datadir / 'glib-2.0' / 'schemas',
)

# Compiled into the build directory for `meson devenv`.
gnome.compile_schemas(build_by_default: true)

test(
	'validate-schema',
	find_program('glib-compile-schemas'),
	args: ['--strict', '--dry-run', meson.current_source_dir()],
)

install_data(
	'icons/hicolor/scalable/apps/' + application_id + '.svg',
	install_dir: datadir / 'icons' / 'hicolor' / 'scalable' / 'apps',
)
install_data(
	'icons/hicolor/symbolic/apps/' + application_id + '-symbolic.svg',
	install_dir: datadir / 'icons' / 'hicolor' / 'symbolic' / 'apps',
)
