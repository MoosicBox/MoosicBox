//! Decoder regressions use the shipped app assets rather than substitute SVGs.

use super::*;
use crate::graphics::{GraphicsLimits, GraphicsSource, SourcePolicy};

fn store() -> GraphicsStore {
    GraphicsStore::new(GraphicsLimits::default())
}

fn decode(store: &mut GraphicsStore, input: &[u8]) -> Result<(), GraphicsError> {
    store.decode(
        "svg",
        &GraphicsSource::Memory,
        &SourcePolicy {
            allow_memory: true,
            ..SourcePolicy::default()
        },
        input,
    )
}

#[test]
fn shipped_app_icons_have_intrinsic_sizes_and_visible_pixels() {
    let mut store = store();
    for (input, dimension) in [
        (
            include_bytes!("../../../../../app/native/public/img/play-button-white.svg").as_slice(),
            800,
        ),
        (
            include_bytes!("../../../../../app/native/public/img/settings-gear.svg").as_slice(),
            800,
        ),
        (
            include_bytes!("../../../../../app/native/public/img/magnifying-glass.svg").as_slice(),
            800,
        ),
        (
            include_bytes!("../../../../../app/native/public/img/no-image-available.svg")
                .as_slice(),
            800,
        ),
        (
            include_bytes!("../../../../../app/native/public/img/previous-button.svg").as_slice(),
            1707,
        ),
    ] {
        decode(&mut store, input).unwrap();
        let resource = &store.resources["svg"];
        assert_eq!((resource.width, resource.height), (dimension, dimension));
        assert!(
            resource
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] != 0)
        );
        assert!(resource.rgba.as_chunks::<4>().0.contains(&[0, 0, 0, 0]));
    }
}

#[test]
fn every_shipped_svg_decodes_with_default_limits() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app/native/public/img");
    let mut store = store();
    let mut count = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "svg") {
            let input = std::fs::read(&path).unwrap();
            assert!(input.len() <= MAX_SVG_BYTES, "{}", path.display());
            decode(&mut store, &input)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(
                store.resources["svg"]
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] == 255),
                "{} must contain opaque artwork",
                path.display()
            );
            count += 1;
        }
    }
    assert!(count >= 45, "expected the real application icon corpus");
}

#[test]
fn isolation_layers_cannot_exceed_the_rgba_allocation_budget() {
    let mut store = GraphicsStore::new(GraphicsLimits {
        max_resource_bytes: 64,
        ..GraphicsLimits::default()
    });
    let input = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><g opacity="0.5"><rect width="1000" height="1000"/></g></svg>"#;
    assert_eq!(decode(&mut store, input), Err(GraphicsError::Limit));
    assert!(store.resources.is_empty());
}

#[test]
fn hostile_finite_isolation_bounds_reject_before_integer_conversion() {
    let mut store = store();
    store
        .register_rgba("svg", 1, 1, vec![1, 2, 3, 255])
        .unwrap();
    let input = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><g opacity="0.5"><rect width="1e30" height="1e30"/></g></svg>"#;
    assert_eq!(decode(&mut store, input), Err(GraphicsError::Limit));
    assert_eq!(store.resources["svg"].rgba.as_ref(), &[1, 2, 3, 255]);
}

#[test]
fn layer_padding_rejects_nonfinite_huge_and_over_budget_dimensions() {
    for value in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        1e30,
        0.0,
        -1.0,
    ] {
        assert_eq!(
            padded_layer_dimension(value, u32::MAX),
            Err(GraphicsError::Limit)
        );
    }
    assert_eq!(
        padded_layer_dimension(4093.0, 4096),
        Err(GraphicsError::Limit)
    );
    assert_eq!(padded_layer_dimension(4092.0, 4096), Ok(4096));
    assert_eq!(padded_layer_dimension(0.5, 4096), Ok(5));
}

#[test]
fn premultiplied_pixels_are_returned_as_straight_alpha() {
    let mut store = store();
    decode(&mut store, br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="1"><path fill="#ff8040" fill-opacity="0.5" d="M0 0h1v1H0z"/></svg>"##).unwrap();
    let pixels = &store.resources["svg"].rgba;
    assert_eq!(&pixels[..4], &[255, 128, 64, 128]);
    assert_eq!(&pixels[4..], &[0, 0, 0, 0]);
}

#[test]
fn viewbox_and_unspecified_intrinsic_size_are_sensible() {
    let mut store = store();
    decode(&mut store, br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M0 0h24v12H0z"/></svg>"#).unwrap();
    assert_eq!(
        (store.resources["svg"].width, store.resources["svg"].height),
        (24, 12)
    );
    decode(&mut store, br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#).unwrap();
    assert_eq!(
        (store.resources["svg"].width, store.resources["svg"].height),
        (100, 100)
    );
}

#[test]
fn hostile_resources_scripts_and_expansion_are_rejected_without_cache_mutation() {
    let mut store = store();
    store
        .register_rgba("svg", 1, 1, vec![1, 2, 3, 255])
        .unwrap();
    for body in [
        r#"<image href="file:///etc/passwd"/>"#,
        r#"<image href="https://example.com/image.png"/>"#,
        r#"<image href="data:image/svg+xml;base64,AAAA"/>"#,
        r"<script>alert(1)</script>",
        r#"<path onload="alert(1)"/>"#,
        r##"<use href="#recursive" id="recursive"/>"##,
        r#"<filter><feGaussianBlur stdDeviation="100000"/></filter>"#,
        r#"<rect fill="url(https://example.com/paint.svg)"/>"#,
        r"<style>@import 'https://example.com/style.css';</style>",
        r"<style>rect { fill: u\72l(file:///etc/passwd); }</style>",
        r"<foreignObject/>",
        r#"<svg width="100000" height="100000"/>"#,
    ] {
        let svg =
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">{body}</svg>"#);
        assert!(decode(&mut store, svg.as_bytes()).is_err(), "{body}");
        assert_eq!(store.resources["svg"].rgba.as_ref(), &[1, 2, 3, 255]);
    }
    for input in [
        b"<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><svg/>".as_slice(),
        b"<svg",
        b"not an image",
        b"\xff<svg/>",
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="1e30" height="1e30"/>"#,
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"/>"#,
    ] {
        assert!(decode(&mut store, input).is_err());
        assert_eq!(store.resources["svg"].rgba.as_ref(), &[1, 2, 3, 255]);
    }
}

#[test]
fn svg_budgets_bound_bytes_dimensions_nodes_depth_and_work() {
    let simple = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#;
    for limits in [
        GraphicsLimits {
            max_dimension: 7,
            ..GraphicsLimits::default()
        },
        GraphicsLimits {
            max_resource_bytes: 255,
            ..GraphicsLimits::default()
        },
        GraphicsLimits {
            max_total_bytes: 255,
            ..GraphicsLimits::default()
        },
        GraphicsLimits {
            max_pixel_visits: 1,
            ..GraphicsLimits::default()
        },
        GraphicsLimits {
            max_actions: 1,
            ..GraphicsLimits::default()
        },
        GraphicsLimits {
            max_encoded_bytes: 1,
            ..GraphicsLimits::default()
        },
    ] {
        assert!(decode(&mut GraphicsStore::new(limits), simple).is_err());
    }
    let mut store = store();
    let nested = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg">{}{}</svg>"#,
        "<g>".repeat(40),
        "</g>".repeat(40)
    );
    assert_eq!(
        decode(&mut store, nested.as_bytes()),
        Err(GraphicsError::Limit)
    );
    assert_eq!(
        decode(&mut store, &vec![b' '; MAX_SVG_BYTES + 1]),
        Err(GraphicsError::Limit)
    );
    let nodes = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg">{}</svg>"#,
        "<path/>".repeat(5000)
    );
    assert_eq!(
        decode(&mut store, nodes.as_bytes()),
        Err(GraphicsError::Limit)
    );
}
