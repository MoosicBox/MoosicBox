#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

pub use color_hex::color_from_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: Option<u8>,
}

impl Color {
    /// Parses a hex string (a-f/A-F/0-9) as a `Color` from the &str,
    /// ignoring surrounding whitespace.
    ///
    /// # Panics
    ///
    /// * If a non-hex, non-whitespace character is encountered.
    #[allow(clippy::many_single_char_names)]
    #[must_use]
    pub fn from_hex(hex: &str) -> Self {
        let mut short_r = 0;
        let mut short_g = 0;
        let mut short_b = 0;
        let mut short_a = 0;
        let mut three_chars = false;
        let mut four_chars = false;

        let mut r = 0;
        let mut g = 0;
        let mut b = 0;
        let mut maybe_a = None;
        let mut a = None;

        let hex = hex.strip_prefix('#').unwrap_or(hex);

        for (i, value) in hex
            .trim()
            .chars()
            .map(|x| match x {
                '0'..='9' => x as u8 - 48,
                'A'..='F' => x as u8 - 55,
                'a'..='f' => x as u8 - 87,
                c if c.is_ascii() => panic!("encountered invalid character: `{x}`"),
                _ => panic!("encountered invalid non-ASCII character"),
            })
            .enumerate()
        {
            match i {
                0 => {
                    short_r = value;
                    r = value << 4;
                }
                1 => {
                    short_g = value;
                    r += value;
                }
                2 => {
                    three_chars = true;
                    short_b = value;
                    g = value << 4;
                }
                3 => {
                    three_chars = false;
                    four_chars = true;
                    short_a = value;
                    g += value;
                }
                4 => {
                    four_chars = false;
                    b = value << 4;
                }
                5 => {
                    b += value;
                }
                6 => {
                    maybe_a = Some(value << 4);
                }
                7 => {
                    a = Some(maybe_a.unwrap() + value);
                }
                _ => {
                    panic!("hex string too long");
                }
            }
        }

        moosicbox_assert::assert_or_panic!(
            maybe_a.is_none() || a.is_some(),
            "hex string invalid length"
        );

        if three_chars {
            r = (short_r << 4) + short_r;
            g = (short_g << 4) + short_g;
            b = (short_b << 4) + short_b;
        }
        if four_chars {
            r = (short_r << 4) + short_r;
            g = (short_g << 4) + short_g;
            b = (short_b << 4) + short_b;
            a = Some((short_a << 4) + short_a);
        }

        Self { r, g, b, a }
    }
}

#[cfg(feature = "egui")]
impl From<Color> for egui::Color32 {
    fn from(value: Color) -> Self {
        value.a.map_or_else(
            || Self::from_rgb(value.r, value.g, value.b),
            |a| Self::from_rgba_unmultiplied(value.r, value.g, value.b, a),
        )
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::Color;

    #[test_log::test]
    fn can_parse_rgb_hex_string_to_color() {
        assert_eq!(
            Color::from_hex("#010203"),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: None
            }
        );
    }

    #[test_log::test]
    fn can_parse_rgba_hex_string_to_color() {
        assert_eq!(
            Color::from_hex("#01020304"),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: Some(4)
            }
        );
    }
}
