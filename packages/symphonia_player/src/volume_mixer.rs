use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
};

pub fn mix_volume(input: &mut AudioBufferRef<'_>, volume: f64) {
    if volume == 1.0 {
        return;
    }
    match input {
        AudioBufferRef::U8(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::U16(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::U24(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::U32(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::S8(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::S16(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::S24(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::S32(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::F32(input) => mix_volume_inner(input.to_mut(), volume),
        AudioBufferRef::F64(input) => mix_volume_inner(input.to_mut(), volume),
    }
}

fn mix_volume_inner<S>(input: &mut AudioBuffer<S>, volume: f64)
where
    S: Sample + FromSample<f32> + IntoSample<f32>,
{
    let channels = input.spec().channels.count();

    for c in 0..channels {
        let src = input.chan_mut(c);
        for x in src {
            let s: f32 = (((*x).into_sample() as f64) * volume) as f32;
            *x = s.into_sample();
        }
    }
}
