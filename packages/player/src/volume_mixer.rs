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
            #[allow(clippy::cast_possible_truncation)]
            let s: f32 = (f64::from((*x).into_sample()) * volume) as f32;
            *x = s.into_sample();
        }
    }
}
