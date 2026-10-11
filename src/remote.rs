//! Download of remote images, when enabled in the preferences.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use soup::prelude::*;

use crate::config;

/// Images larger than this are not downloaded.
const MAX_IMAGE_SIZE: usize = 20 * 1024 * 1024;
const TIMEOUT_SECONDS: u32 = 15;
const READ_CHUNK_SIZE: usize = 64 * 1024;

thread_local! {
	static SESSION: soup::Session = {
		let session = soup::Session::new();
		session.set_timeout(TIMEOUT_SECONDS);
		session.set_user_agent(&format!("Lectern/{}", config::VERSION));
		session
	};
	/// Images downloaded during the session, by URL.
	static CACHE: RefCell<HashMap<String, gdk::Texture>> = RefCell::new(HashMap::new());
}

/// Whether `url` designates a remote image that can be downloaded.
pub fn is_remote(url: &str) -> bool {
	matches!(
		glib::Uri::peek_scheme(url).as_deref(),
		Some("http" | "https")
	)
}

/// Downloads and decodes the image at `url`, or returns it from the cache.
pub async fn texture(url: &str) -> Option<gdk::Texture> {
	if let Some(texture) = CACHE.with(|cache| cache.borrow().get(url).cloned()) {
		return Some(texture);
	}
	let bytes = download(url).await?;
	// Decode in a thread: large images would freeze the interface.
	let texture = gio::spawn_blocking(move || gdk::Texture::from_bytes(&bytes))
		.await
		.ok()?
		.ok()?;
	CACHE.with(|cache| cache.borrow_mut().insert(url.to_owned(), texture.clone()));
	Some(texture)
}

async fn download(url: &str) -> Option<glib::Bytes> {
	let message = soup::Message::new("GET", url).ok()?;
	let session = SESSION.with(Clone::clone);
	let stream = session
		.send_future(&message, glib::Priority::DEFAULT)
		.await
		.ok()?;
	if message.status() != soup::Status::Ok {
		return None;
	}
	let announced = message
		.response_headers()
		.map_or(-1, |headers| headers.content_length());
	if usize::try_from(announced).is_ok_and(|length| length > MAX_IMAGE_SIZE) {
		return None;
	}

	// The announced length may be missing or wrong: enforce the limit while reading.
	let mut data = Vec::new();
	loop {
		let chunk = stream
			.read_bytes_future(READ_CHUNK_SIZE, glib::Priority::DEFAULT)
			.await
			.ok()?;
		if chunk.is_empty() {
			break;
		}
		data.extend_from_slice(&chunk);
		if data.len() > MAX_IMAGE_SIZE {
			return None;
		}
	}
	Some(glib::Bytes::from_owned(data))
}
