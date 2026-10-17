//! Qualification rules shared by the build script and negative tests.

use std::fmt::Write as _;

/// Required public reference source revision.
pub const REVISION: &str = "34bba701ae97c913de719b1f7c10686f62cddb15";

/// Validates source identity and effective generated configuration.
///
/// # Errors
/// * Rejects a different revision, tracked modifications, or incompatible configuration.
pub fn qualify(revision: &str, clean: bool, config: &str, cache: &str) -> Result<(), String> {
    if revision != REVISION || !clean {
        return Err("reference source revision/cleanliness mismatch".into());
    }
    let defined = |name: &str| {
        config.lines().any(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("#define") && words.next() == Some(name)
        })
    };
    if !defined("FIXED_POINT")
        || defined("ENABLE_RES24")
        || defined("ENABLE_QEXT")
        || !defined("DISABLE_FLOAT_API")
    {
        return Err("reference requires fixed-point, no RES24/QEXT, no float API".into());
    }
    for expected in [
        "OPUS_FIXED_POINT:BOOL=ON",
        "OPUS_DISABLE_INTRINSICS:BOOL=ON",
        "OPUS_ENABLE_FLOAT_API:BOOL=OFF",
    ] {
        if !cache.lines().any(|line| line == expected) {
            return Err(format!("missing effective CMake setting: {expected}"));
        }
    }
    Ok(())
}

/// Validates effective compiler definitions as well as generated configuration.
///
/// # Errors
/// * Rejects missing required defines, overrides/undefines, or enabled SIMD/RES24/QEXT.
pub fn qualify_command(config: &str, cache: &str, command: &str) -> Result<String, String> {
    let mut effective = config.to_owned();
    for word in command.split_whitespace() {
        if word.starts_with("-U") || word.starts_with("/U") {
            return Err("compiler undefines are not qualified".into());
        }
        if let Some(definition) = word.strip_prefix("-D").or_else(|| word.strip_prefix("/D")) {
            let definition = definition.trim_matches(['\'', '"']);
            let (name, value) = definition.split_once('=').unwrap_or((definition, "1"));
            if (name == "FIXED_POINT" && value != "1")
                || name.starts_with("OPUS_X86_")
                || name.starts_with("OPUS_ARM_")
            {
                return Err(format!("unqualified compiler definition: {definition}"));
            }
            let _ = write!(effective, "\n#define {name} {value}\n");
        }
    }
    qualify(REVISION, true, &effective, cache)?;
    Ok(effective)
}

#[cfg(test)]
mod tests {
    use super::*;
    const CONFIG: &str = "#define FIXED_POINT 1\n#define DISABLE_FLOAT_API 1\n";
    const CACHE: &str = "OPUS_FIXED_POINT:BOOL=ON\nOPUS_DISABLE_INTRINSICS:BOOL=ON\nOPUS_ENABLE_FLOAT_API:BOOL=OFF\n";
    #[test]
    fn rejects_unqualified_reference() {
        assert!(qualify(REVISION, true, CONFIG, CACHE).is_ok());
        assert!(qualify("wrong", true, CONFIG, CACHE).is_err());
        assert!(qualify(REVISION, false, CONFIG, CACHE).is_err());
        for name in ["ENABLE_RES24", "ENABLE_QEXT"] {
            assert!(
                qualify(
                    REVISION,
                    true,
                    &format!("{CONFIG}#define {name} 1\n"),
                    CACHE
                )
                .is_err()
            );
        }
        assert!(
            qualify(
                REVISION,
                true,
                CONFIG,
                &CACHE.replace("INTRINSICS:BOOL=ON", "INTRINSICS:BOOL=OFF")
            )
            .is_err()
        );
        for command in [
            "-DFIXED_POINT=0 -DDISABLE_FLOAT_API",
            "-DFIXED_POINT=1 -DDISABLE_FLOAT_API -DENABLE_RES24",
            "-DFIXED_POINT=1 -DDISABLE_FLOAT_API -DENABLE_QEXT",
            "-DFIXED_POINT=1 -DDISABLE_FLOAT_API -UENABLE_RES24",
            "-DFIXED_POINT=1 -DDISABLE_FLOAT_API -DOPUS_X86_MAY_HAVE_SSE",
        ] {
            assert!(qualify_command("", CACHE, command).is_err());
        }
        assert!(qualify_command("", CACHE, "cc -DFIXED_POINT=1 -DDISABLE_FLOAT_API").is_ok());
        assert!(qualify(REVISION, true, "", CACHE).is_err());
    }
}
