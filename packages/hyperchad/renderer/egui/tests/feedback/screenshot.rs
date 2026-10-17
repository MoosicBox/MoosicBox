//! Explicit screenshot acceptance: normal runs never create or update baselines.
//!
//! Artifacts are always written as `actual.png`; a mismatch also writes
//! `expected.png` and `diff.png`. Baseline review/copy is a separate operation.

use std::{fmt, fs, path::Path};

use image::{Rgba, RgbaImage};

#[derive(Debug)]
pub enum ScreenshotError {
    Io(std::io::Error),
    Image(image::ImageError),
    MissingBaseline,
    Mismatch { pixels: u64 },
}

impl fmt::Display for ScreenshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "screenshot artifact I/O failed: {error}"),
            Self::Image(error) => write!(f, "screenshot image I/O failed: {error}"),
            Self::MissingBaseline => write!(f, "missing reviewed baseline; inspect actual.png"),
            Self::Mismatch { pixels } => {
                write!(
                    f,
                    "{pixels} screenshot pixels differ; inspect expected/actual/diff.png"
                )
            }
        }
    }
}

impl std::error::Error for ScreenshotError {}

impl From<std::io::Error> for ScreenshotError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<image::ImageError> for ScreenshotError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

/// Compare a captured production frame against an explicitly reviewed baseline.
///
/// `channel_tolerance` is the maximum allowed absolute difference in any RGBA
/// channel. Set it to zero for the pinned software-rendering environment. There
/// is no allowed pixel-count budget: a single pixel exceeding tolerance fails.
///
/// # Errors
/// * Returns `Io` or `Image` if artifacts cannot be read or written.
/// * Returns `MissingBaseline` without accepting a new baseline.
/// * Returns `Mismatch` for different dimensions or pixels beyond tolerance.
pub fn compare(
    actual: &RgbaImage,
    baseline: &Path,
    artifacts: &Path,
    channel_tolerance: u8,
) -> Result<(), ScreenshotError> {
    if ["actual.png", "expected.png", "diff.png"]
        .iter()
        .any(|name| aliases(baseline, &artifacts.join(name)))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "baseline aliases a mutable screenshot artifact",
        )
        .into());
    }
    // Read the reviewed baseline before any artifact mutation, including when
    // a caller provides a symlinked artifact directory.
    let expected = match image::open(baseline) {
        Ok(image) => Some(image.into_rgba8()),
        Err(image::ImageError::IoError(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            None
        }
        Err(error) => return Err(error.into()),
    };
    fs::create_dir_all(artifacts)?;
    // Remove stale comparison evidence from a previous invocation. Never remove
    // the actual image or the caller's baseline.
    for name in ["expected.png", "diff.png"] {
        match fs::remove_file(artifacts.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    actual.save(artifacts.join("actual.png"))?;
    let Some(expected) = expected else {
        return Err(ScreenshotError::MissingBaseline);
    };
    expected.save(artifacts.join("expected.png"))?;

    let width = actual.width().max(expected.width());
    let height = actual.height().max(expected.height());
    let mut diff = RgbaImage::new(width, height);
    let mut pixels = 0;
    for (x, y, pixel) in diff.enumerate_pixels_mut() {
        let actual_pixel = actual.get_pixel_checked(x, y);
        let expected_pixel = expected.get_pixel_checked(x, y);
        let mismatch = match (actual_pixel, expected_pixel) {
            (Some(actual), Some(expected)) => actual
                .0
                .iter()
                .zip(expected.0)
                .any(|(a, e)| a.abs_diff(e) > channel_tolerance),
            _ => true,
        };
        if mismatch {
            pixels += 1;
            *pixel = Rgba([255, 0, 255, 255]);
        } else {
            *pixel = Rgba([0, 0, 0, 255]);
        }
    }
    if pixels == 0 {
        return Ok(());
    }
    diff.save(artifacts.join("diff.png"))?;
    Err(ScreenshotError::Mismatch { pixels })
}

fn aliases(first: &Path, second: &Path) -> bool {
    fn resolved(path: &Path) -> std::path::PathBuf {
        if let Ok(path) = path.canonicalize() {
            return path;
        }
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
            && let Ok(parent) = parent.canonicalize()
        {
            return parent.join(name);
        }
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        let mut normalized = std::path::PathBuf::new();
        for component in absolute.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                component => normalized.push(component.as_os_str()),
            }
        }
        normalized
    }
    if resolved(first) == resolved(second) {
        return true;
    }
    #[cfg(unix)]
    if let (Ok(first), Ok(second)) = (fs::metadata(first), fs::metadata(second)) {
        use std::os::unix::fs::MetadataExt;
        return first.dev() == second.dev() && first.ino() == second.ino();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // Kept inside the granted tree. The process ID avoids collisions between
    // concurrent invocations; each case below has its own artifact directory.
    fn root(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../target/feedback-artifacts/comparator")
            .join(std::process::id().to_string())
            .join(name)
    }

    #[test]
    fn baseline_aliases_are_rejected_without_mutation() {
        let root = root("aliases");
        fs::create_dir_all(&root).unwrap();
        let baseline = root.join("actual.png");
        let expected = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        expected.save(&baseline).unwrap();
        let actual = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        for path in [baseline.clone(), root.join("./actual.png")] {
            assert!(matches!(
                compare(&actual, &path, &root, 0),
                Err(ScreenshotError::Io(_))
            ));
            assert_eq!(image::open(&baseline).unwrap().into_rgba8(), expected);
        }
        #[cfg(unix)]
        {
            let link = root.join("reviewed.png");
            if !link.exists() {
                std::os::unix::fs::symlink(&baseline, &link).unwrap();
            }
            assert!(compare(&actual, &link, &root, 0).is_err());
            assert_eq!(image::open(&baseline).unwrap().into_rgba8(), expected);
            let directory_link = root.join("alias-directory");
            if !directory_link.exists() {
                std::os::unix::fs::symlink(&root, &directory_link).unwrap();
            }
            assert!(compare(&actual, &baseline, &directory_link, 0).is_err());
        }
    }

    #[test]
    fn intentional_failure_writes_expected_actual_and_diff() {
        let root = root("intentional-failure");
        fs::create_dir_all(&root).unwrap();
        let baseline = root.join("baseline.png");
        let expected = RgbaImage::from_pixel(8, 8, Rgba([20, 30, 40, 255]));
        expected.save(&baseline).unwrap();
        let mut actual = expected.clone();
        actual.put_pixel(3, 4, Rgba([255, 30, 40, 255]));
        let artifacts = root.join("comparison");
        assert!(matches!(
            compare(&actual, &baseline, &artifacts, 0),
            Err(ScreenshotError::Mismatch { pixels: 1 })
        ));
        for name in ["expected.png", "actual.png", "diff.png"] {
            assert!(artifacts.join(name).is_file(), "missing {name}");
        }
        let diff = image::open(artifacts.join("diff.png"))
            .unwrap()
            .into_rgba8();
        assert_eq!(*diff.get_pixel(3, 4), Rgba([255, 0, 255, 255]));
        assert_eq!(*diff.get_pixel(0, 0), Rgba([0, 0, 0, 255]));
        assert_eq!(image::open(&baseline).unwrap().into_rgba8(), expected);
        // A subsequent successful run must not leave a stale failure diff.
        compare(&expected, &baseline, &artifacts, 0).unwrap();
        assert!(!artifacts.join("diff.png").exists());
        // Restore the intentional failure evidence for inspection after testing.
        assert!(compare(&actual, &baseline, &artifacts, 0).is_err());
        eprintln!(
            "intentional screenshot failure proof: {}",
            artifacts.display()
        );
    }

    #[test]
    fn missing_baseline_is_not_auto_accepted() {
        let root = root("missing-baseline");
        let baseline = root.join("absent.png");
        assert!(!baseline.exists());
        let actual = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        assert!(matches!(
            compare(&actual, &baseline, &root.join("comparison"), 0),
            Err(ScreenshotError::MissingBaseline)
        ));
        assert!(!baseline.exists());
        assert!(root.join("comparison/actual.png").is_file());
    }

    #[test]
    fn dimension_changes_fail_even_with_maximum_channel_tolerance() {
        let root = root("dimensions");
        fs::create_dir_all(&root).unwrap();
        let baseline = root.join("baseline.png");
        RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]))
            .save(&baseline)
            .unwrap();
        let actual = RgbaImage::from_pixel(3, 2, Rgba([0, 0, 0, 255]));
        assert!(matches!(
            compare(&actual, &baseline, &root.join("comparison"), u8::MAX),
            Err(ScreenshotError::Mismatch { pixels: 2 })
        ));
    }
}
