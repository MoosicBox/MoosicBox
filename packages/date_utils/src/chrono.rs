pub use chrono::*;

/// # Errors
///
/// * If the datetime fails to parse
pub fn parse_date_time(value: &str) -> Result<NaiveDateTime, chrono::ParseError> {
    if value.len() <= 4 {
        if let Ok(year) = value.parse::<u16>() {
            if let Some(date) = NaiveDate::default().with_year(i32::from(year)) {
                return Ok(date.into());
            }
        }
    }
    if value.len() == 10 {
        return NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .inspect_err(|&e| {
                log::error!("Error parsing 10 {value}: {e:?}");
            })
            .map(Into::into);
    }
    if value.ends_with('Z') {
        return NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ").inspect_err(|&e| {
            log::error!("Error parsing full z {value}: {e:?}");
        });
    }
    if value.ends_with("+00:00") {
        return NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z").inspect_err(|&e| {
            log::error!("Error parsing full %.f%z {value}: {e:?}");
        });
    }

    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").inspect_err(|&e| {
        log::error!("Error parsing full {value}: {e:?}");
    })
}
