use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio;
use gtk::glib;

const WIN64: &[u8] = include_bytes!("../assets/icons/win64.svg");
const WIN32: &[u8] = include_bytes!("../assets/icons/win32.svg");
const LINUX: &[u8] = include_bytes!("../assets/icons/linux.svg");
const BSD: &[u8] = include_bytes!("../assets/icons/bsd.svg");
const OTHER: &[u8] = include_bytes!("../assets/icons/other.svg");

fn bytes_for(os_type: &str) -> &'static [u8] {
    match os_type {
        "win64" => WIN64,
        "win32" => WIN32,
        "linux" => LINUX,
        "bsd" => BSD,
        _ => OTHER,
    }
}

pub fn os_pixbuf(os_type: &str, size: i32) -> Option<Pixbuf> {
    let bytes = glib::Bytes::from_static(bytes_for(os_type));
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    Pixbuf::from_stream_at_scale(&stream, size, size, true, gio::Cancellable::NONE).ok()
}
