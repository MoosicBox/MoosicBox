use actix_web::error::ErrorBadRequest;
use thiserror::Error;

use crate::sqlite::models::Id;

#[derive(Debug, Error)]
pub enum ParseIntegersError {
    #[error("Could not parse integers: {0}")]
    ParseId(String),
    #[error("Unmatched range: {0}")]
    UnmatchedRange(String),
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
}

impl From<ParseIntegersError> for actix_web::Error {
    fn from(err: ParseIntegersError) -> Self {
        match err {
            ParseIntegersError::ParseId(id) => {
                ErrorBadRequest(format!("Could not parse integers '{id}'"))
            }
            ParseIntegersError::UnmatchedRange(range) => {
                ErrorBadRequest(format!("Unmatched range '{range}'"))
            }
            ParseIntegersError::RangeTooLarge(range) => {
                ErrorBadRequest(format!("Range too large '{range}'"))
            }
        }
    }
}

pub fn parse_integer_sequences(
    integers: &str,
) -> std::result::Result<Vec<u64>, ParseIntegersError> {
    integers
        .split(',')
        .map(|id| {
            id.parse::<u64>()
                .map_err(|_| ParseIntegersError::ParseId(id.into()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
}

pub fn parse_integer_ranges(
    integer_ranges: &str,
) -> std::result::Result<Vec<u64>, ParseIntegersError> {
    let ranges = integer_ranges.split('-').collect::<Vec<_>>();

    if ranges.len() == 1 {
        parse_integer_sequences(ranges[0])
    } else if ranges.len() > 2 && ranges.len() % 2 == 1 {
        Err(ParseIntegersError::UnmatchedRange(integer_ranges.into()))
    } else {
        let mut i = 0;
        let mut ids = Vec::new();

        while i < ranges.len() {
            let mut start = parse_integer_sequences(ranges[i])?;
            let mut start_id = start[start.len() - 1] + 1;
            let mut end = parse_integer_sequences(ranges[i + 1])?;
            let end_id = end[0];

            if end_id - start_id > 100000 {
                return Err(ParseIntegersError::RangeTooLarge(format!(
                    "{}-{}",
                    start_id - 1,
                    end_id,
                )));
            }

            ids.append(&mut start);

            while start_id < end_id {
                ids.push(start_id);
                start_id += 1;
            }

            ids.append(&mut end);

            i += 2;
        }

        Ok(ids)
    }
}

pub fn parse_integer_ranges_to_ids(
    integer_ranges: &str,
) -> std::result::Result<Vec<Id>, ParseIntegersError> {
    Ok(parse_integer_ranges(integer_ranges)?
        .into_iter()
        .map(|x| x.into())
        .collect::<Vec<Id>>())
}
