use symphonia::core::{
    audio::{AudioBuffer, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
};

pub fn mix_volume<S>(input: &mut AudioBuffer<S>, volume: f64)
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
