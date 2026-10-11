project(
	'lectern',
	version: '0.1.0',
	license: 'MIT',
	meson_version: '>= 1.4.0',
)

i18n = import('i18n')
gnome = import('gnome')

application_id = 'com.skunkcrafts.Lectern'
gettext_package = meson.project_name()

prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
datadir = prefix / get_option('datadir')
localedir = prefix / get_option('localedir')

dependency('gtk4', version: '>= 4.18')
dependency('libadwaita-1', version: '>= 1.7')
dependency('gtksourceview-5', version: '>= 5.10')
dependency('libsoup-3.0', version: '>= 3.0')
find_program('blueprint-compiler')

subdir('data')
subdir('src')
subdir('po')

devenv = environment()
devenv.set('GSETTINGS_SCHEMA_DIR', meson.project_build_root() / 'data')
devenv.set('LECTERN_LOCALEDIR', meson.project_build_root() / 'po')
devenv.prepend('PATH', meson.project_build_root() / 'src')
meson.add_devenv(devenv)

gnome.post_install(
	glib_compile_schemas: true,
	gtk_update_icon_cache: true,
	update_desktop_database: true,
)
