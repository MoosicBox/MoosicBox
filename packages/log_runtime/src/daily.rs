//! Daily files rotate on worker writes, using UTC rather than producer time.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use time::{Date, OffsetDateTime};

pub struct DailyFile {
    dir: PathBuf,
    prefix: String,
    day: Date,
    file: File,
}

impl DailyFile {
    pub(super) fn new(dir: &Path, prefix: &str) -> Self {
        let day = OffsetDateTime::now_utc().date();
        Self {
            dir: dir.into(),
            prefix: prefix.into(),
            day,
            file: open(dir, prefix, day).expect("initializing rolling file appender failed"),
        }
    }

    fn rotate(&mut self, day: Date) {
        if day <= self.day {
            return;
        }
        // Advance even on open failure: keep writing the old file until the next day.
        self.day = day;
        match open(&self.dir, &self.prefix, day) {
            Ok(next) => {
                if let Err(err) = self.file.flush() {
                    eprintln!("Couldn't flush previous writer: {err}");
                }
                self.file = next;
            }
            Err(err) => eprintln!("Couldn't create writer for logs: {err}"),
        }
    }
}

fn open(dir: &Path, prefix: &str, day: Date) -> io::Result<File> {
    let name = if prefix.is_empty() {
        day.to_string()
    } else {
        format!("{prefix}.{day}")
    };
    let path = dir.join(name);
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    options.open(&path).or_else(|_| {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        options.open(path)
    })
}

impl Write for DailyFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.rotate(OffsetDateTime::now_utc().date());
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prefix_and_failed_roll_forward() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = DailyFile::new(dir.path(), "");
        let today = sink.day;
        let tomorrow = today.next_day().unwrap();
        let blocked = dir.path().join(tomorrow.to_string());
        std::fs::create_dir(&blocked).unwrap();
        sink.rotate(tomorrow);
        std::fs::remove_dir(&blocked).unwrap();
        sink.rotate(tomorrow); // no retry on the failed date
        sink.file.write_all(b"fallback").unwrap();
        let later = tomorrow.next_day().unwrap();
        sink.rotate(later);
        sink.rotate(today); // backwards clock must not reopen old file
        sink.file.write_all(b"new").unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(today.to_string())).unwrap(),
            b"fallback"
        );
        assert!(!blocked.exists());
        assert_eq!(
            std::fs::read(dir.path().join(later.to_string())).unwrap(),
            b"new"
        );
    }
}
