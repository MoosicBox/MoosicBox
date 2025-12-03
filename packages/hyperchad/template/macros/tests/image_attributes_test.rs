#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template::container;
use hyperchad_transformer::Element;
use hyperchad_transformer_models::{ImageFit, ImageLoading};

#[test_log::test]
fn test_image_alt_text() {
    let containers = container! {
        image src="/photo.jpg" alt="A beautiful sunset" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { source, alt, .. } = &containers[0].element {
        assert_eq!(source.as_deref(), Some("/photo.jpg"));
        assert_eq!(alt.as_deref(), Some("A beautiful sunset"));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_dynamic_alt() {
    let alt_text = "Dynamic alt text";

    let containers = container! {
        image src="/photo.jpg" alt=(alt_text) {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { alt, .. } = &containers[0].element {
        assert_eq!(alt.as_deref(), Some("Dynamic alt text"));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_loading_lazy() {
    let containers = container! {
        image src="/photo.jpg" loading="lazy" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { loading, .. } = &containers[0].element {
        assert_eq!(*loading, Some(ImageLoading::Lazy));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_loading_eager() {
    let containers = container! {
        image src="/photo.jpg" loading="eager" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { loading, .. } = &containers[0].element {
        assert_eq!(*loading, Some(ImageLoading::Eager));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_fit_contain() {
    let containers = container! {
        image src="/photo.jpg" fit="contain" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(*fit, Some(ImageFit::Contain));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_fit_cover() {
    let containers = container! {
        image src="/photo.jpg" fit="cover" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(*fit, Some(ImageFit::Cover));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_fit_fill() {
    let containers = container! {
        image src="/photo.jpg" fit="fill" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(*fit, Some(ImageFit::Fill));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_fit_none() {
    let containers = container! {
        image src="/photo.jpg" fit="none" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(*fit, Some(ImageFit::None));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_fit_default() {
    let containers = container! {
        image src="/photo.jpg" fit="default" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(*fit, Some(ImageFit::Default));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_all_attributes() {
    let containers = container! {
        image
            src="/photo.jpg"
            alt="Full featured image"
            srcset="photo-2x.jpg 2x, photo-3x.jpg 3x"
            loading="lazy"
            fit="cover"
        {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image {
        source,
        alt,
        source_set,
        loading,
        fit,
        ..
    } = &containers[0].element
    {
        assert_eq!(source.as_deref(), Some("/photo.jpg"));
        assert_eq!(alt.as_deref(), Some("Full featured image"));
        assert_eq!(
            source_set.as_deref(),
            Some("photo-2x.jpg 2x, photo-3x.jpg 3x")
        );
        assert_eq!(*loading, Some(ImageLoading::Lazy));
        assert_eq!(*fit, Some(ImageFit::Cover));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_image_dynamic_src() {
    fn get_image_url() -> String {
        "/dynamic/image.png".to_string()
    }

    let containers = container! {
        image src=(get_image_url()) alt="Dynamic source" {}
    };

    assert_eq!(containers.len(), 1);

    if let Element::Image { source, .. } = &containers[0].element {
        assert_eq!(source.as_deref(), Some("/dynamic/image.png"));
    } else {
        panic!("Expected Image element, got: {:?}", containers[0].element);
    }
}
