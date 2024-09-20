use thiserror::Error;

use crate::sqlite::models::{ApiSource, Id, IdType};

#[derive(Debug, Error)]
pub enum ParseIntegersError {
    #[error("Could not parse integers: {0}")]
    ParseId(String),
    #[error("Unmatched range: {0}")]
    UnmatchedRange(String),
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
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

#[derive(Debug, Error)]
pub enum ParseIdsError {
    #[error("Could not parse ids: {0}")]
    ParseId(String),
    #[error("Unmatched range: {0}")]
    UnmatchedRange(String),
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
}

pub fn parse_id_sequences(
    ids: &str,
    source: ApiSource,
    id_type: IdType,
) -> std::result::Result<Vec<Id>, ParseIdsError> {
    ids.split(',')
        .map(|id| {
            Id::try_from_str(id, source, id_type).map_err(|_| ParseIdsError::ParseId(id.into()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
}

pub fn parse_id_ranges(
    id_ranges: &str,
    source: ApiSource,
    id_type: IdType,
) -> std::result::Result<Vec<Id>, ParseIdsError> {
    let default = Id::default_value(source, id_type);
    let ranges = if default.is_number() {
        id_ranges.split('-').collect::<Vec<_>>()
    } else {
        vec![id_ranges]
    };

    if ranges.len() == 1 {
        parse_id_sequences(ranges[0], source, id_type)
    } else if ranges.len() > 2 && ranges.len() % 2 == 1 {
        Err(ParseIdsError::UnmatchedRange(id_ranges.into()))
    } else {
        let mut i = 0;
        let mut ids = Vec::new();

        while i < ranges.len() {
            let mut start = parse_id_sequences(ranges[i], source, id_type)?;
            let start_id = start[start.len() - 1].clone();
            let mut end = parse_id_sequences(ranges[i + 1], source, id_type)?;
            let end_id = end[0].clone();

            ids.append(&mut start);

            if let Id::Number(end_id) = end_id {
                if let Id::Number(mut start_id) = start_id {
                    start_id += 1;

                    if end_id - start_id > 100000 {
                        return Err(ParseIdsError::RangeTooLarge(format!(
                            "{}-{}",
                            start_id - 1,
                            end_id,
                        )));
                    }

                    while start_id < end_id {
                        ids.push(Id::Number(start_id));
                        start_id += 1;
                    }
                }
            }

            ids.append(&mut end);

            i += 2;
        }

        Ok(ids)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::{sqlite::models::Id, *};

    use self::sqlite::models::{ApiSource, IdType};

    use super::parse_id_ranges;

    #[test_log::test]
    fn can_parse_number_track_id_ranges() {
        let result = parse_id_ranges("1,2,3,5-10,450", ApiSource::Library, IdType::Track).unwrap();

        assert_eq!(
            result,
            vec![
                Id::Number(1),
                Id::Number(2),
                Id::Number(3),
                Id::Number(5),
                Id::Number(6),
                Id::Number(7),
                Id::Number(8),
                Id::Number(9),
                Id::Number(10),
                Id::Number(450),
            ]
        );
    }

    #[test_log::test]
    fn can_parse_string_track_id_ranges() {
        let result = parse_id_ranges("a,b,aaa,bbb,c-d,f", ApiSource::Yt, IdType::Track).unwrap();

        assert_eq!(
            result,
            vec![
                Id::String("a".into()),
                Id::String("b".into()),
                Id::String("aaa".into()),
                Id::String("bbb".into()),
                Id::String("c-d".into()),
                Id::String("f".into()),
            ]
        );
    }
}
