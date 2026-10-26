//! Folder contents and how Finder words them: kinds, sizes, dates.

use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub kind: String,
}

impl Entry {
    /// An entry from what the index knows (search results): creation and
    /// access times are left for `from_path`.
    pub fn from_parts(
        name: String,
        path: PathBuf,
        is_dir: bool,
        size: u64,
        modified: Option<SystemTime>,
    ) -> Entry {
        let kind = kind_of(&name, is_dir).into();
        Entry {
            name: name.into(),
            path,
            is_dir,
            size: if is_dir { 0 } else { size },
            modified,
            created: None,
            accessed: None,
            kind,
        }
    }
}

/// Display the root or the last component of a path.
pub fn display_name(path: &Path) -> String {
    match path.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => "File System".into(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Modified,
    Size,
    Kind,
}

/// Finder's order: the key, then name (natural, case-insensitive).
pub fn sort(v: &mut [Entry], key: SortKey, ascending: bool) {
    v.sort_by(|a, b| {
        let o = match key {
            SortKey::Name => std::cmp::Ordering::Equal,
            SortKey::Modified => a.modified.cmp(&b.modified),
            SortKey::Size => a.size.cmp(&b.size).then(b.is_dir.cmp(&a.is_dir)),
            SortKey::Kind => kind_rank(a)
                .cmp(&kind_rank(b))
                .then_with(|| a.kind.cmp(&b.kind)),
        };
        let o = o.then_with(|| natural_cmp(&a.name, &b.name));
        if ascending { o } else { o.reverse() }
    });
}

fn kind_rank(e: &Entry) -> u8 {
    if e.is_dir { 0 } else { 1 }
}

/// "file2" before "file10", ignoring case.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek(), y.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let n: String = std::iter::from_fn(|| x.next_if(|c| c.is_ascii_digit())).collect();
                let m: String = std::iter::from_fn(|| y.next_if(|c| c.is_ascii_digit())).collect();
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if o.is_ne() {
                    return o;
                }
            }
            (Some(_), Some(_)) => {
                let (c, d) = (x.next().unwrap(), y.next().unwrap());
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o.is_ne() {
                    return o;
                }
            }
        }
    }
}

fn ext(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() => e.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Finder's "Kind" column wording.
pub fn kind_of(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "Folder".into();
    }
    let e = ext(name);
    let k = match e.as_str() {
        "png" => "PNG image",
        "jpg" | "jpeg" => "JPEG image",
        "gif" => "GIF Image",
        "heic" => "HEIC image",
        "webp" => "WebP image",
        "svg" => "SVG document",
        "bmp" => "BMP image",
        "ico" => "Windows icon",
        "psd" => "Adobe Photoshop file",
        "mp4" | "m4v" => "MPEG-4 movie",
        "mov" => "QuickTime movie",
        "mkv" => "Matroska video",
        "avi" => "AVI movie",
        "webm" => "WebM video",
        "mp3" => "MP3 audio",
        "m4a" => "Apple MPEG-4 audio",
        "wav" => "Waveform audio",
        "flac" => "FLAC audio",
        "pdf" => "PDF document",
        "txt" => "Plain Text",
        "md" => "Markdown document",
        "rtf" => "Rich Text Document",
        "doc" | "docx" => "Microsoft Word document",
        "xls" | "xlsx" => "Microsoft Excel spreadsheet",
        "ppt" | "pptx" => "Microsoft PowerPoint presentation",
        "csv" => "CSV document",
        "json" => "JSON",
        "html" | "htm" => "HTML text",
        "css" => "CSS",
        "js" | "mjs" => "JavaScript script",
        "ts" | "tsx" => "TypeScript",
        "rs" => "Rust source",
        "py" => "Python script",
        "c" => "C source",
        "h" => "C header",
        "cpp" | "cc" => "C++ source",
        "go" => "Go source",
        "java" => "Java source",
        "swift" => "Swift source",
        "sh" => "Shell script",
        "ps1" => "PowerShell script",
        "bat" | "cmd" => "Batch file",
        "toml" => "TOML document",
        "yaml" | "yml" => "YAML document",
        "xml" => "XML text",
        "zip" => "ZIP archive",
        "7z" => "7-Zip archive",
        "rar" => "RAR archive",
        "gz" | "tgz" => "gzip compressed archive",
        "tar" => "tar archive",
        "iso" => "Disk image",
        "dmg" => "Apple disk image",
        "exe" => "Application",
        "msi" => "Windows Installer package",
        "lnk" => "Alias",
        "dll" => "Dynamic library",
        "ttf" | "otf" => "Font",
        "" => "Document",
        _ => return format!("{} file", e.to_ascii_uppercase()),
    };
    k.into()
}

/// Free bytes on the volume holding `path`.

/// Finder's sizes: decimal units, whole KB, one decimal from MB up.
pub fn format_size(e: &Entry) -> String {
    if e.is_dir || e.kind == "Application" {
        return "--".into();
    }
    format_bytes(e.size)
}

pub fn format_bytes(b: u64) -> String {
    if b == 0 {
        return "Zero bytes".into();
    }
    if b < 1000 {
        return format!("{b} bytes");
    }
    if b < 1_000_000 {
        return format!("{} KB", (b + 500) / 1000);
    }
    let units = ["MB", "GB", "TB"];
    let mut v = b as f64 / 1e6;
    let mut u = 0;
    while v >= 1000. && u < units.len() - 1 {
        v /= 1000.;
        u += 1;
    }
    format!("{v:.1} {}", units[u])
}

/// "Today at 5:15 PM", "Yesterday at 9:02 AM", "Sep 20, 2026 at 8:17 PM".
pub fn format_date(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return "--".into() };
    let d: DateTime<Local> = t.into();
    let today = Local::now().date_naive();
    let time = d.format("%-I:%M %p");
    if d.date_naive() == today {
        format!("Today at {time}")
    } else if Some(d.date_naive()) == today.pred_opt() {
        format!("Yesterday at {time}")
    } else {
        format!("{} at {time}", d.format("%b %-d, %Y"))
    }
}

/// "Saturday, September 6, 2025 at 2:49 PM": the inspector's long form.
pub fn format_date_long(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return "--".into() };
    let d: DateTime<Local> = t.into();
    d.format("%A, %B %-d, %Y at %-I:%M %p").to_string()
}
