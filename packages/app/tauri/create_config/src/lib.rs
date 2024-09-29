#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{io::Write as _, path::Path};

use serde::Serialize;

#[derive(Serialize)]
pub struct Config {
    pub web: bool,
    pub app: bool,
    pub bundled: bool,
}

impl Config {
    pub fn to_json(&self) -> String {
        let json = serde_json::to_string(self).unwrap();

        format!("export const config = {json} as const;",)
    }
}

pub fn gen<P: AsRef<Path>>(bundled: bool, output: P) {
    let config = Config {
        web: false,
        app: true,
        bundled,
    };

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output)
        .unwrap();

    file.write_all(config.to_json().as_bytes()).unwrap();
}
