//! Embeds the exe icon and version info (Windows), and decodes the window icon
//! to raw RGBA so the app carries no PNG decoder.

use std::{env, fs, path::PathBuf};

const WINDOW_ICON: &str = "assets/icon-64.png";

fn main() {
    println!("cargo:rerun-if-changed={WINDOW_ICON}");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    decode_window_icon();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Claude Limit Checker")
            .set("FileDescription", "Claude Limit Checker")
            .set("OriginalFilename", "ClaudeLimitChecker.exe");
        res.compile().expect("embed the Windows resources");
    }
}

/// Writes `$OUT_DIR/icon-64.rgba`: 64 x 64 straight-alpha RGBA8.
fn decode_window_icon() {
    let file = fs::File::open(WINDOW_ICON).expect("open the window icon");
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("read the window icon");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("decode the window icon");
    assert_eq!(
        (info.width, info.height),
        (64, 64),
        "{WINDOW_ICON} must be 64x64"
    );
    let pixels = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => panic!("unexpected icon color type {other:?}"),
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("icon-64.rgba");
    fs::write(out, rgba).expect("write the decoded icon");
}
