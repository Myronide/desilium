//! Writes real XCursor files and theme directories into temp dirs for the tests.

use std::fs;
use std::path::Path;

/// One image chunk of a cursor file; the image is a solid square of `argb`.
pub(crate) struct FrameSpec {
    pub size: u32,
    pub delay_ms: u32,
    pub hotspot: (u32, u32),
    pub argb: u32,
}

pub(crate) fn frame(size: u32, delay_ms: u32, hotspot: (u32, u32), argb: u32) -> FrameSpec {
    FrameSpec {
        size,
        delay_ms,
        hotspot,
        argb,
    }
}

const MAGIC: &[u8; 4] = b"Xcur";
const FILE_HEADER_LEN: u32 = 16;
const FILE_VERSION: u32 = 0x0001_0000;
const TOC_ENTRY_LEN: u32 = 12;
const IMAGE_CHUNK_TYPE: u32 = 0xfffd_0002;
const IMAGE_CHUNK_HEADER_LEN: u32 = 36;
const IMAGE_CHUNK_VERSION: u32 = 1;

/// Encodes frames as a valid XCursor file: header, table of contents, then image chunks.
pub(crate) fn encode_cursor_file(frames: &[FrameSpec]) -> Vec<u8> {
    let entry_count = u32::try_from(frames.len()).unwrap_or(u32::MAX);
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    push_u32(&mut out, FILE_HEADER_LEN);
    push_u32(&mut out, FILE_VERSION);
    push_u32(&mut out, entry_count);

    let mut position = FILE_HEADER_LEN + entry_count * TOC_ENTRY_LEN;
    for spec in frames {
        push_u32(&mut out, IMAGE_CHUNK_TYPE);
        push_u32(&mut out, spec.size);
        push_u32(&mut out, position);
        position += IMAGE_CHUNK_HEADER_LEN + spec.size * spec.size * 4;
    }
    for spec in frames {
        push_u32(&mut out, IMAGE_CHUNK_HEADER_LEN);
        push_u32(&mut out, IMAGE_CHUNK_TYPE);
        push_u32(&mut out, spec.size);
        push_u32(&mut out, IMAGE_CHUNK_VERSION);
        push_u32(&mut out, spec.size);
        push_u32(&mut out, spec.size);
        push_u32(&mut out, spec.hotspot.0);
        push_u32(&mut out, spec.hotspot.1);
        push_u32(&mut out, spec.delay_ms);
        for _ in 0..spec.size * spec.size {
            push_u32(&mut out, spec.argb);
        }
    }
    out
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Creates `<root>/<name>/index.theme` (with an `Inherits=` line) and its `cursors/` files.
pub(crate) fn write_theme(
    root: &Path,
    name: &str,
    inherits: &[&str],
    cursors: &[(&str, Vec<FrameSpec>)],
) {
    let theme_dir = root.join(name);
    let cursor_dir = theme_dir.join("cursors");
    assert!(fs::create_dir_all(&cursor_dir).is_ok(), "create cursor dir");
    let index = format!(
        "[Icon Theme]\nName={name}\nInherits={}\n",
        inherits.join(",")
    );
    assert!(
        fs::write(theme_dir.join("index.theme"), index).is_ok(),
        "write index.theme"
    );
    for (file, frames) in cursors {
        assert!(
            fs::write(cursor_dir.join(file), encode_cursor_file(frames)).is_ok(),
            "write cursor"
        );
    }
}
