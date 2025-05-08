pub trait TimeFormat {
    fn into_formatted(self) -> String;
}

impl TimeFormat for u32 {
    fn into_formatted(self) -> String {
        u128::from(self).into_formatted()
    }
}

impl TimeFormat for u64 {
    fn into_formatted(self) -> String {
        u128::from(self).into_formatted()
    }
}

impl TimeFormat for u128 {
    fn into_formatted(self) -> String {
        #[must_use]
        const fn plural(num: u128) -> &'static str {
            if num == 1 { "" } else { "s" }
        }

        let years = self / 365 / 24 / 60 / 60 / 1000;
        let days = self / 24 / 60 / 60 / 1000 % 365;
        let hours = self / 60 / 60 / 1000 % 24;
        let minutes = self / 60 / 1000 % 60;
        let seconds = self / 1000 % 60;
        let ms = self % 1000;

        if years > 0 {
            format!(
                "{years} year{}, {days} day{}, {hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(years),
                plural(days),
                plural(hours),
                plural(minutes),
            )
        } else if days > 0 {
            format!(
                "{days} day{}, {hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(days),
                plural(hours),
                plural(minutes),
            )
        } else if hours > 0 {
            format!(
                "{hours} hour{}, {minutes} minute{}, {seconds}s, {ms}ms",
                plural(hours),
                plural(minutes),
            )
        } else if minutes > 0 {
            format!("{minutes} minute{}, {seconds}s, {ms}ms", plural(minutes))
        } else if seconds > 0 {
            format!("{seconds}s, {ms}ms")
        } else {
            format!("{ms}ms")
        }
    }
}
