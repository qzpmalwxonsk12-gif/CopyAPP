use crate::models::{CapturedClipboard, StoredEntry};

fn extract_html_fragment(raw: String) -> String {
    let offset = |name: &str| {
        raw.lines().find_map(|line| {
            line.strip_prefix(name)
                .and_then(|value| value.strip_prefix(':'))
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
    };
    if let (Some(start), Some(end)) = (offset("StartFragment"), offset("EndFragment")) {
        if start < end {
            if let Some(fragment) = raw.get(start..end) {
                return fragment.to_string();
            }
        }
    }

    const START_MARKER: &str = "<!--StartFragment-->";
    const END_MARKER: &str = "<!--EndFragment-->";
    if let Some(start) = raw.find(START_MARKER) {
        let fragment_start = start + START_MARKER.len();
        if let Some(relative_end) = raw[fragment_start..].find(END_MARKER) {
            return raw[fragment_start..fragment_start + relative_end].to_string();
        }
    }
    raw
}

pub enum WatcherReady {
    Native(MonitorShutdown),
    Fallback(String),
}

#[cfg(windows)]
mod imp {
    use std::{
        io::Cursor,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::SyncSender,
            Arc,
        },
        thread,
        time::Duration,
    };

    use clipboard_win::{formats, options::NoClear, Clipboard, Getter};
    use image::ImageFormat;
    use sha2::{Digest, Sha256};

    use super::{extract_html_fragment, CapturedClipboard, StoredEntry};

    pub struct MonitorShutdown(Option<clipboard_win::monitor::Shutdown>);

    impl MonitorShutdown {
        pub fn stop(mut self) {
            self.0.take();
        }
    }

    pub fn run_change_monitor<F>(
        shutdown: Arc<AtomicBool>,
        ready: SyncSender<super::WatcherReady>,
        mut changed: F,
    ) -> Result<(), String>
    where
        F: FnMut() + Send + 'static,
    {
        match clipboard_win::Monitor::new() {
            Ok(mut monitor) => {
                let stop = monitor.shutdown_channel();
                ready
                    .send(super::WatcherReady::Native(MonitorShutdown(Some(stop))))
                    .map_err(|_| "clipboard watcher startup receiver closed".to_string())?;
                while !shutdown.load(Ordering::Acquire) {
                    match monitor.recv() {
                        Ok(true) => changed(),
                        Ok(false) => break,
                        Err(error) => return Err(format!("native clipboard listener: {error}")),
                    }
                }
                Ok(())
            }
            Err(error) => {
                ready
                    .send(super::WatcherReady::Fallback(format!(
                        "native clipboard listener unavailable: {error}"
                    )))
                    .map_err(|_| "clipboard watcher startup receiver closed".to_string())?;
                let mut sequence = sequence_number();
                while !shutdown.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(250));
                    let next = sequence_number();
                    if next != 0 && next != sequence {
                        sequence = next;
                        changed();
                    }
                }
                Ok(())
            }
        }
    }

    pub fn sequence_number() -> u32 {
        clipboard_win::raw::seq_num().map(|n| n.get()).unwrap_or(0)
    }

    pub fn capture() -> Result<Option<CapturedClipboard>, String> {
        let _guard = Clipboard::new_attempts(12).map_err(|e| format!("open clipboard: {e}"))?;

        let mut text = String::new();
        if formats::Unicode.read_clipboard(&mut text).is_err() {
            text.clear();
        }
        while text.ends_with('\0') {
            text.pop();
        }
        let text = (!text.is_empty()).then_some(text);

        let mut html = String::new();
        if let Some(format) = formats::Html::new() {
            if format.read_clipboard(&mut html).is_err() {
                html.clear();
            }
        }
        while html.ends_with('\0') {
            html.pop();
        }
        let html = (!html.is_empty()).then(|| extract_html_fragment(html));

        let mut file_paths = Vec::<String>::new();
        if formats::FileList.read_clipboard(&mut file_paths).is_err() {
            file_paths.clear();
        }

        let mut bitmap = Vec::new();
        if formats::Bitmap.read_clipboard(&mut bitmap).is_err() {
            bitmap.clear();
        }
        let (image_png, thumbnail_png) = if bitmap.is_empty() {
            (None, None)
        } else {
            match image::load_from_memory_with_format(&bitmap, ImageFormat::Bmp) {
                Ok(image) => {
                    let mut full = Cursor::new(Vec::new());
                    image
                        .write_to(&mut full, ImageFormat::Png)
                        .map_err(|e| format!("encode clipboard image: {e}"))?;
                    let thumb_image = image.thumbnail(360, 220);
                    let mut thumb = Cursor::new(Vec::new());
                    thumb_image
                        .write_to(&mut thumb, ImageFormat::Png)
                        .map_err(|e| format!("encode image preview: {e}"))?;
                    (Some(full.into_inner()), Some(thumb.into_inner()))
                }
                Err(_) => (None, None),
            }
        };

        if text.is_none() && html.is_none() && image_png.is_none() && file_paths.is_empty() {
            return Ok(None);
        }

        let kind = if !file_paths.is_empty() {
            "files"
        } else if image_png.is_some() {
            "image"
        } else if html.is_some() {
            "html"
        } else {
            "text"
        }
        .to_string();

        let hash = fingerprint(
            text.as_deref(),
            html.as_deref(),
            image_png.as_deref(),
            &file_paths,
        );
        let byte_size = text.as_ref().map_or(0, |v| v.len())
            + html.as_ref().map_or(0, |v| v.len())
            + image_png.as_ref().map_or(0, |v| v.len())
            + file_paths.iter().map(String::len).sum::<usize>();
        Ok(Some(CapturedClipboard {
            kind,
            text,
            html,
            image_png,
            thumbnail_png,
            file_paths,
            hash,
            byte_size: byte_size.min(i64::MAX as usize) as i64,
        }))
    }

    pub fn write(entry: &StoredEntry) -> Result<(), String> {
        let _guard = Clipboard::new_attempts(12).map_err(|e| format!("open clipboard: {e}"))?;
        clipboard_win::raw::empty().map_err(|e| format!("clear clipboard for paste: {e}"))?;
        let mut formats_written = 0usize;
        let mut failures = Vec::new();

        if let Some(text) = entry.text.as_deref() {
            match clipboard_win::raw::set_string_with(text, NoClear) {
                Ok(()) => formats_written += 1,
                Err(e) => failures.push(format!("text: {e}")),
            }
        }
        if let Some(html) = entry.html.as_deref() {
            if let Some(format) = formats::Html::new() {
                match clipboard_win::raw::set_html_with(format.code(), html, NoClear) {
                    Ok(()) => formats_written += 1,
                    Err(e) => failures.push(format!("HTML: {e}")),
                }
            }
        }
        if let Some(png) = entry.image_png.as_deref() {
            match png_to_bmp(png).and_then(|bmp| {
                clipboard_win::raw::set_bitmap_with(&bmp, NoClear)
                    .map_err(|e| format!("write image: {e}"))
            }) {
                Ok(()) => formats_written += 1,
                Err(e) => failures.push(e),
            }
        }
        if !entry.file_paths.is_empty() {
            match clipboard_win::raw::set_file_list_with(&entry.file_paths, NoClear) {
                Ok(()) => formats_written += 1,
                Err(e) => failures.push(format!("files: {e}")),
            }
        }

        if formats_written == 0 {
            Err(if failures.is_empty() {
                "clipboard item has no writable payload".into()
            } else {
                failures.join("; ")
            })
        } else {
            Ok(())
        }
    }

    fn png_to_bmp(png: &[u8]) -> Result<Vec<u8>, String> {
        let image = image::load_from_memory_with_format(png, ImageFormat::Png)
            .map_err(|e| format!("decode stored image: {e}"))?;
        let mut bmp = Cursor::new(Vec::new());
        image
            .write_to(&mut bmp, ImageFormat::Bmp)
            .map_err(|e| format!("prepare image for clipboard: {e}"))?;
        Ok(bmp.into_inner())
    }

    fn fingerprint(
        text: Option<&str>,
        html: Option<&str>,
        image: Option<&[u8]>,
        files: &[String],
    ) -> String {
        let mut hasher = Sha256::new();
        if let Some(value) = text {
            hasher.update(b"text\0");
            hasher.update(value.as_bytes());
        }
        if let Some(value) = html {
            hasher.update(b"html\0");
            hasher.update(value.as_bytes());
        }
        if let Some(value) = image {
            hasher.update(b"image\0");
            hasher.update(value);
        }
        for file in files {
            hasher.update(b"file\0");
            hasher.update(file.to_lowercase().as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{CapturedClipboard, StoredEntry};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::SyncSender,
            Arc,
        },
        thread,
        time::Duration,
    };
    pub struct MonitorShutdown;
    impl MonitorShutdown {
        pub fn stop(self) {}
    }
    pub fn run_change_monitor<F>(
        shutdown: Arc<AtomicBool>,
        ready: SyncSender<super::WatcherReady>,
        mut changed: F,
    ) -> Result<(), String>
    where
        F: FnMut() + Send + 'static,
    {
        let _ = ready.send(super::WatcherReady::Fallback(
            "native clipboard listener is available on Windows only".into(),
        ));
        while !shutdown.load(Ordering::Acquire) {
            thread::sleep(Duration::from_secs(1));
            changed();
        }
        Ok(())
    }
    pub fn sequence_number() -> u32 {
        0
    }
    pub fn capture() -> Result<Option<CapturedClipboard>, String> {
        Ok(None)
    }
    pub fn write(_entry: &StoredEntry) -> Result<(), String> {
        Err("native clipboard access is available on Windows only".into())
    }
}

pub use imp::{capture, run_change_monitor, write, MonitorShutdown};

#[cfg(test)]
mod tests {
    use super::extract_html_fragment;

    #[test]
    fn extracts_cf_html_with_space_padded_offsets() {
        let fragment = "<strong>CopyAPP rich verification</strong>";
        let mut header = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000      \r\nEndFragment:0000000000      \r\n".to_string();
        let body_prefix = "<html><body><!--StartFragment-->";
        let body_suffix = "<!--EndFragment--></body></html>";
        let start = header.len() + body_prefix.len();
        let end = start + fragment.len();
        header = header.replacen(
            "StartFragment:0000000000",
            &format!("StartFragment:{start:010}"),
            1,
        );
        header = header.replacen(
            "EndFragment:0000000000",
            &format!("EndFragment:{end:010}"),
            1,
        );
        let raw = format!("{header}{body_prefix}{fragment}{body_suffix}");
        assert_eq!(extract_html_fragment(raw), fragment);
    }

    #[test]
    fn falls_back_to_fragment_markers() {
        let raw = "<html><!--StartFragment--><em>hello</em><!--EndFragment--></html>";
        assert_eq!(extract_html_fragment(raw.into()), "<em>hello</em>");
    }
}
