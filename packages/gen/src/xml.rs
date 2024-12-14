#![allow(clippy::module_name_repetitions)]

use quickcheck::{Arbitrary, Gen};

#[derive(Clone, Debug)]
pub struct XmlString(pub String);

impl Arbitrary for XmlString {
    fn arbitrary(g: &mut Gen) -> Self {
        let string = loop {
            let string = String::arbitrary(g);
            if !string
                .chars()
                .any(|x| matches!(x, '\u{0000}'..='\u{001F}' | '\u{FFFE}'..='\u{FFFF}'))
            {
                break string;
            }
        };

        Self(string)
    }
}
