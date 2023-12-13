use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseTrackIdsError {
    #[error("Could not parse trackId: {0}")]
    ParseId(String),
    #[error("Unmatched range: {0}")]
    UnmatchedRange(String),
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
}

pub fn parse_track_id_sequences(
    track_ids: &str,
) -> std::result::Result<Vec<i32>, ParseTrackIdsError> {
    track_ids
        .split(',')
        .map(|id| {
            id.parse::<i32>()
                .map_err(|_| ParseTrackIdsError::ParseId(id.into()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
}

pub fn parse_track_id_ranges(
    track_id_ranges: &str,
) -> std::result::Result<Vec<i32>, ParseTrackIdsError> {
    let ranges = track_id_ranges.split('-').collect::<Vec<_>>();

    if ranges.len() == 1 {
        parse_track_id_sequences(ranges[0])
    } else if ranges.len() > 2 && ranges.len() % 2 == 1 {
        Err(ParseTrackIdsError::UnmatchedRange(track_id_ranges.into()))
    } else {
        let mut i = 0;
        let mut ids = Vec::new();

        while i < ranges.len() {
            let mut start = parse_track_id_sequences(ranges[i])?;
            let mut start_id = start[start.len() - 1] + 1;
            let mut end = parse_track_id_sequences(ranges[i + 1])?;
            let end_id = end[0];

            if end_id - start_id > 100000 {
                return Err(ParseTrackIdsError::RangeTooLarge(format!(
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
