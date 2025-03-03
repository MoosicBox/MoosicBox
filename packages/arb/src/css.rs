#![allow(clippy::module_name_repetitions)]

use quickcheck::{Arbitrary, Gen};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CssIdentifierString(pub String);

impl Arbitrary for CssIdentifierString {
    fn arbitrary(g: &mut Gen) -> Self {
        let string = loop {
            let string = String::arbitrary(g);
            if !string.is_empty()
                && string
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '-' | '_'))
                && !string.chars().all(|c| matches!(c, '-' | '_'))
            {
                break string;
            }
        };

        Self(string)
    }
}
