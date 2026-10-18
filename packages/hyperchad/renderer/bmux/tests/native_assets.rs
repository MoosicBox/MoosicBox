//! Real native assets and exact decoded pixels; no terminal or network required.
use std::io::Cursor;

use bmux_tui::geometry::Rect;
use hyperchad_renderer_bmux::graphics::{
    GraphicsFit, GraphicsPlacement, GraphicsSource, GraphicsStore, SourcePolicy,
};

#[test]
fn png_decodes_exact_rgba_and_keeps_destination_and_clip() {
    let pixels = vec![
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let image = image::RgbaImage::from_raw(2, 2, pixels.clone()).unwrap();
    let mut encoded = Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let mut store = GraphicsStore::new(Default::default());
    let policy = SourcePolicy {
        allow_memory: true,
        ..SourcePolicy::default()
    };
    store
        .decode("art", &GraphicsSource::Memory, &policy, encoded.get_ref())
        .unwrap();
    assert_eq!(store.rgba("art"), Some((2, 2, pixels.as_slice())));
    let destination = Rect::new(7, 3, 4, 2);
    let clip = Rect::new(8, 3, 2, 2);
    let placed = store
        .placement(
            "art",
            "cover",
            destination,
            clip,
            GraphicsFit::Stretch,
            (8.0, 16.0),
            true,
            "cover",
        )
        .unwrap();
    let GraphicsPlacement::Image(placement) = placed else {
        panic!("decoded artwork was replaced by text")
    };
    assert_eq!(placement.destination, clip);
    let allocation = placement.allocation.unwrap();
    assert_eq!(allocation.x, i32::from(destination.x));
    assert_eq!(allocation.y, i32::from(destination.y));
    assert_eq!(placement.clip, clip);
    assert_eq!(store.rgba("art").unwrap().2, pixels);
}

#[test]
fn every_native_svg_decodes_nonempty_and_white_icons_preserve_color() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app/native/public/img");
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "svg"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 45,
        "native asset inventory unexpectedly incomplete"
    );
    let policy = SourcePolicy {
        allow_memory: true,
        ..SourcePolicy::default()
    };
    for file in files {
        let mut store = GraphicsStore::new(Default::default());
        let encoded = std::fs::read(&file).unwrap();
        store
            .decode("icon", &GraphicsSource::Memory, &policy, &encoded)
            .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        let (width, height, pixels) = store.rgba("icon").unwrap();
        assert!(width > 0 && height > 0);
        assert!(
            pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0),
            "transparent icon: {}",
            file.display()
        );
        if file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-white.svg")
        {
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel.iter().all(|channel| *channel > 200)),
                "white icon turned dark: {}",
                file.display()
            );
        }
    }
}
