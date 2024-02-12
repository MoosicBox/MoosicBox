use libpulse_binding as pulse;

use log::warn;

use symphonia::core::audio::Channels;

/// Maps a set of Symphonia `Channels` to a PulseAudio channel map.
pub fn map_channels_to_pa_channelmap(channels: Channels) -> Option<pulse::channelmap::Map> {
    let mut map: pulse::channelmap::Map = Default::default();
    map.init();
    map.set_len(channels.count() as u8);

    let is_mono = channels.count() == 1;

    for (i, channel) in channels.iter().enumerate() {
        map.get_mut()[i] = match channel {
            Channels::FRONT_LEFT if is_mono => pulse::channelmap::Position::Mono,
            Channels::FRONT_LEFT => pulse::channelmap::Position::FrontLeft,
            Channels::FRONT_RIGHT => pulse::channelmap::Position::FrontRight,
            Channels::FRONT_CENTRE => pulse::channelmap::Position::FrontCenter,
            Channels::REAR_LEFT => pulse::channelmap::Position::RearLeft,
            Channels::REAR_CENTRE => pulse::channelmap::Position::RearCenter,
            Channels::REAR_RIGHT => pulse::channelmap::Position::RearRight,
            Channels::LFE1 => pulse::channelmap::Position::Lfe,
            Channels::FRONT_LEFT_CENTRE => pulse::channelmap::Position::FrontLeftOfCenter,
            Channels::FRONT_RIGHT_CENTRE => pulse::channelmap::Position::FrontRightOfCenter,
            Channels::SIDE_LEFT => pulse::channelmap::Position::SideLeft,
            Channels::SIDE_RIGHT => pulse::channelmap::Position::SideRight,
            Channels::TOP_CENTRE => pulse::channelmap::Position::TopCenter,
            Channels::TOP_FRONT_LEFT => pulse::channelmap::Position::TopFrontLeft,
            Channels::TOP_FRONT_CENTRE => pulse::channelmap::Position::TopFrontCenter,
            Channels::TOP_FRONT_RIGHT => pulse::channelmap::Position::TopFrontRight,
            Channels::TOP_REAR_LEFT => pulse::channelmap::Position::TopRearLeft,
            Channels::TOP_REAR_CENTRE => pulse::channelmap::Position::TopRearCenter,
            Channels::TOP_REAR_RIGHT => pulse::channelmap::Position::TopRearRight,
            _ => {
                // If a Symphonia channel cannot map to a PulseAudio position then return None
                // because PulseAudio will not be able to open a stream with invalid channels.
                warn!("failed to map channel {:?} to output", channel);
                return None;
            }
        }
    }

    Some(map)
}
