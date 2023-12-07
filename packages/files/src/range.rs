use thiserror::Error;

pub struct Range {
    pub start: Option<usize>,
    pub end: Option<usize>,
}

#[derive(Debug, Error)]
pub enum ParseRangesError {
    #[error("Could not parse range value: {0}")]
    Parse(String),
    #[error("Too few range values: {0}")]
    TooFewValues(String),
    #[error("Too many range values: {0}")]
    TooManyValues(String),
}

pub fn parse_range(range: &str) -> std::result::Result<Range, ParseRangesError> {
    let ends = range
        .split('-')
        .map(|id| {
            if id.is_empty() {
                Ok(None)
            } else {
                Some(
                    id.parse::<usize>()
                        .map_err(|_| ParseRangesError::Parse(id.into())),
                )
                .transpose()
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;

    match ends.len() {
        2 => Ok(Range {
            start: ends[0],
            end: ends[1],
        }),
        0 | 1 => Err(ParseRangesError::TooFewValues(range.to_string())),
        _ => Err(ParseRangesError::TooManyValues(range.to_string())),
    }
}

pub fn parse_ranges(ranges: &str) -> std::result::Result<Vec<Range>, ParseRangesError> {
    ranges.split(',').map(parse_range).collect()
}
