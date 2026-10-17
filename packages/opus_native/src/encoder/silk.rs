//! SILK encoder: integer LPC codebook search and closed-loop excitation coding.
//! This first SILK path uses unvoiced prediction (no pitch/LTP) and preserves
//! reconstructed LPC history across packets. It is not a voiced speech encoder.
use super::{EncoderConfig, entropy};
use crate::range::RangeEncoder;
use crate::silk::{self, SilkDecoder};
use crate::{Bandwidth, Error, Result, SampleRate};

#[derive(Clone)]
pub(super) struct SilkEncoder {
    config: EncoderConfig,
    history: [[i32; 16]; 2],
    gain: [i32; 2],
    previous_index: [i32; 2],
}

struct Model {
    stage1: u8,
    residual: Vec<i8>,
    lpc: Vec<i16>,
}

impl SilkEncoder {
    pub(super) fn new(config: EncoderConfig) -> Result<Self> {
        let rate = sample_rate(config.sample_rate)?;
        if ![rate / 100, rate / 50].contains(&config.frame_size)
            || !(8_000..=128_000).contains(&config.bitrate)
        {
            return Err(Error::Unsupported(
                "SILK encoding requires 8/12/16 kHz, 10/20 ms and 8..=128 kbit/s".into(),
            ));
        }
        Ok(Self {
            config,
            history: [[0; 16]; 2],
            gain: [65536; 2],
            previous_index: [10; 2],
        })
    }

    pub(super) fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        self.encode_internal(pcm, None)
    }

    #[cfg(feature = "hybrid")]
    pub(super) fn encode_prefix(&mut self, pcm: &[f32], encoder: &mut RangeEncoder) -> Result<()> {
        self.encode_internal(pcm, Some(encoder)).map(|_| ())
    }

    #[allow(clippy::too_many_lines)] // Transactional SILK entropy budget search.
    fn encode_internal(
        &mut self,
        pcm: &[f32],
        mut shared: Option<&mut RangeEncoder>,
    ) -> Result<Vec<u8>> {
        let rate = sample_rate(self.config.sample_rate)?;
        let bandwidth = bandwidth(self.config.sample_rate);
        let channels = super::channel_count(self.config.channels);
        let mut inputs = vec![Vec::with_capacity(self.config.frame_size); channels];
        for frame in pcm.chunks_exact(channels) {
            let left = (f64::from(frame[0]) * 32768.0).round() as i32;
            if channels == 1 {
                inputs[0].push(left);
            } else {
                let right = (f64::from(frame[1]) * 32768.0).round() as i32;
                inputs[0].push(left.midpoint(right));
                inputs[1].push((left - right) / 2);
            }
        }
        let models: Vec<_> = inputs
            .iter()
            .enumerate()
            .map(|(channel, input)| {
                let mut analysis: Vec<i32> = self.history[channel]
                    .iter()
                    .map(|&x| {
                        ((i64::from(x) * i64::from(self.gain[channel])) >> 30).clamp(-32768, 32767)
                            as i32
                    })
                    .collect();
                analysis.extend_from_slice(input);
                analyze(&analysis, bandwidth)
            })
            .collect::<Result<_>>()?;
        let bytes =
            (self.config.bitrate as usize * self.config.frame_size / (rate * 8)).saturating_sub(1);
        // Actual entropy budgeting with transactional reconstruction. Raising
        // the common gain reduces excitation density without packet truncation.
        let mut last_failure = String::new();
        for trial in 0..64 {
            let indices: Vec<_> = (0..channels)
                .map(|c| trial.max(self.previous_index[c] - 16).min(63))
                .collect();
            let gains: Vec<_> = indices
                .iter()
                .map(|&index| SilkDecoder::dequantize_gain(index))
                .collect();
            let quantized: Vec<_> = (0..channels)
                .map(|c| {
                    quantize(
                        &inputs[c],
                        &models[c].lpc,
                        &self.history[c],
                        self.gain[c],
                        gains[c],
                    )
                })
                .collect();
            let encode = |encoder: &mut RangeEncoder| -> Result<()> {
                for _ in 0..channels {
                    encoder.ec_enc_bit_logp(true, 1).map_err(entropy)?;
                    encoder.ec_enc_bit_logp(false, 1).map_err(entropy)?;
                }
                if channels == 2 {
                    use silk::constants::{
                        STEREO_WEIGHT_PDF_STAGE1, STEREO_WEIGHT_PDF_STAGE2,
                        STEREO_WEIGHT_PDF_STAGE3,
                    };
                    // Exact zero stereo predictors: both coarse indices select
                    // the -820..820 interval and its central interpolation.
                    encoder
                        .ec_enc_icdf(12, STEREO_WEIGHT_PDF_STAGE1, 8)
                        .map_err(entropy)?;
                    for _ in 0..2 {
                        encoder
                            .ec_enc_icdf(1, STEREO_WEIGHT_PDF_STAGE2, 8)
                            .map_err(entropy)?;
                        encoder
                            .ec_enc_icdf(2, STEREO_WEIGHT_PDF_STAGE3, 8)
                            .map_err(entropy)?;
                    }
                }
                for c in 0..channels {
                    encode_parameters(
                        encoder,
                        &models[c],
                        indices[c],
                        self.previous_index[c],
                        self.config.frame_size == rate / 50,
                        bandwidth,
                    )?;
                    encode_pulses(encoder, &quantized[c].0)?;
                }
                Ok(())
            };
            let mut encoder = RangeEncoder::new(bytes).map_err(entropy)?;
            if let Err(error) = encode(&mut encoder) {
                last_failure = format!(
                    "trial {trial}: {error}; pulses max={:?} sum={:?} gains={gains:?}",
                    quantized
                        .iter()
                        .map(|(q, _)| q.iter().map(|q| q.unsigned_abs()).max())
                        .collect::<Vec<_>>(),
                    quantized
                        .iter()
                        .map(|(q, _)| q.iter().map(|q| u64::from(q.unsigned_abs())).sum::<u64>())
                        .collect::<Vec<_>>()
                );
                continue;
            }
            if encoder.tell() as usize > bytes * 8 {
                last_failure = format!("trial {trial}: {} bits > {}", encoder.tell(), bytes * 8);
                continue;
            }
            if let Some(shared) = shared.as_deref_mut() {
                encode(shared)?;
                for c in 0..channels {
                    self.history[c] = quantized[c].1;
                    self.gain[c] = gains[c];
                    self.previous_index[c] = indices[c];
                }
                return Ok(Vec::new());
            }
            // SILK-only Opus implicitly interprets extra entropy bytes as CELT
            // redundancy. Re-encode at measured size instead of truncating.
            let exact_bytes = (encoder.tell() as usize).div_ceil(8);
            let mut encoder = RangeEncoder::new(exact_bytes).map_err(entropy)?;
            encode(&mut encoder)?;
            if let Ok(payload) = encoder.finish() {
                for c in 0..channels {
                    self.history[c] = quantized[c].1;
                    self.gain[c] = gains[c];
                    self.previous_index[c] = indices[c];
                }
                let config = match bandwidth {
                    Bandwidth::Narrowband => 0,
                    Bandwidth::Mediumband => 4,
                    _ => 8,
                } + u8::from(self.config.frame_size == rate / 50);
                let mut packet = vec![(config << 3) | if channels == 2 { 4 } else { 0 }];
                packet.extend(payload);
                return Ok(packet);
            }
        }
        Err(Error::Unsupported(format!(
            "SILK excitation cannot fit packet bitrate budget: {last_failure}"
        )))
    }
}

fn sample_rate(rate: SampleRate) -> Result<usize> {
    match rate {
        SampleRate::Hz8000 => Ok(8000),
        SampleRate::Hz12000 => Ok(12000),
        SampleRate::Hz16000 => Ok(16000),
        _ => Err(Error::Unsupported(
            "SILK encoder internal rate must be 8/12/16 kHz".into(),
        )),
    }
}
const fn bandwidth(rate: SampleRate) -> Bandwidth {
    match rate {
        SampleRate::Hz8000 => Bandwidth::Narrowband,
        SampleRate::Hz12000 => Bandwidth::Mediumband,
        _ => Bandwidth::Wideband,
    }
}

fn prediction_error(input: &[i32], lpc: &[i16]) -> i128 {
    input
        .iter()
        .enumerate()
        .map(|(i, &sample)| {
            let prediction = lpc
                .iter()
                .enumerate()
                .take(i)
                .map(|(j, &a)| i64::from(a) * i64::from(input[i - j - 1]))
                .sum::<i64>()
                >> 12;
            let error = i64::from(sample) - prediction;
            i128::from(error) * i128::from(error)
        })
        .sum()
}

// Search the actual quantized LPC filters rather than an unquantized model.
// Coordinate refinement changes every NLSF residual and recomputes stability-
// limited coefficients, so encoder synthesis uses exactly the transmitted model.
fn analyze(input: &[i32], bandwidth: Bandwidth) -> Result<Model> {
    let order = if bandwidth == Bandwidth::Wideband {
        16
    } else {
        10
    };
    let mut best = None;
    let mut best_error = i128::MAX;
    for stage1 in 0..32 {
        let residual = vec![0; order];
        let lsf = SilkDecoder::reconstruct_lsf(stage1, &residual, bandwidth)?;
        let lpc = SilkDecoder::limit_lpc_coefficients(&lsf, bandwidth)?;
        let error = prediction_error(input, &lpc);
        if error < best_error {
            best_error = error;
            best = Some(Model {
                stage1,
                residual,
                lpc,
            });
        }
    }
    let mut model = best.ok_or_else(|| Error::Unsupported("empty SILK LPC codebook".into()))?;
    for step in [2_i8, 1] {
        for coordinate in 0..order {
            let original = model.residual[coordinate];
            for candidate in [original - step, original + step] {
                if !(-3..=3).contains(&candidate) {
                    continue;
                }
                let mut residual = model.residual.clone();
                residual[coordinate] = candidate;
                let lsf = SilkDecoder::reconstruct_lsf(model.stage1, &residual, bandwidth)?;
                let lpc = SilkDecoder::limit_lpc_coefficients(&lsf, bandwidth)?;
                let error = prediction_error(input, &lpc);
                if error < best_error {
                    best_error = error;
                    model.residual = residual;
                    model.lpc = lpc;
                }
            }
        }
    }
    Ok(model)
}

fn quantize(
    input: &[i32],
    lpc: &[i16],
    history: &[i32; 16],
    old_gain: i32,
    gain: i32,
) -> (Vec<i16>, [i32; 16]) {
    let mut history = *history;
    for value in &mut history {
        *value = (i64::from(*value) * i64::from(old_gain) / i64::from(gain)) as i32;
    }
    let mut seed = 0_u32;
    let mut pulses = Vec::with_capacity(input.len().next_multiple_of(16));
    for &sample in input {
        let prediction = ((lpc.len() / 2) as i32
            + lpc
                .iter()
                .enumerate()
                .map(|(j, &a)| ((i64::from(history[15 - j]) * i64::from(a)) >> 16) as i32)
                .sum::<i32>())
            << 4;
        let target = (i64::from(sample) * (1_i64 << 30) / i64::from(gain)) as i32;
        seed = seed.wrapping_mul(196_314_165).wrapping_add(907_633_515);
        let sign = if seed & 0x8000_0000 == 0 { 1 } else { -1 };
        let residual = target.saturating_sub(prediction) * sign;
        let pulse = ((residual - 1600 + if residual >= 1600 { 1280 } else { -1280 } + 8192) >> 14)
            .clamp(-127, 127) as i16;
        let mut excitation = i32::from(pulse) * 16384;
        if pulse > 0 {
            excitation -= 1280;
        } else if pulse < 0 {
            excitation += 1280;
        }
        excitation = (excitation + 1600) * sign; // Unvoiced/low quantization offset.
        seed = seed.wrapping_add(pulse as u32);
        let reconstructed = prediction.saturating_add(excitation);
        history.copy_within(1.., 0);
        history[15] = reconstructed;
        pulses.push(pulse);
    }
    pulses.resize(input.len().next_multiple_of(16), 0);
    (pulses, history)
}

fn encode_parameters(
    enc: &mut RangeEncoder,
    model: &Model,
    gain: i32,
    previous_gain: i32,
    long: bool,
    bandwidth: Bandwidth,
) -> Result<()> {
    use silk::constants::{FRAME_TYPE_PDF_ACTIVE, GAIN_PDF_DELTA, GAIN_PDF_LSB, GAIN_PDF_UNVOICED};
    enc.ec_enc_icdf(0, FRAME_TYPE_PDF_ACTIVE, 8)
        .map_err(entropy)?; // Unvoiced, low offset.
    let coded = gain.max(previous_gain - 16);
    enc.ec_enc_icdf((coded >> 3) as usize, GAIN_PDF_UNVOICED, 8)
        .map_err(entropy)?;
    enc.ec_enc_icdf((coded & 7) as usize, GAIN_PDF_LSB, 8)
        .map_err(entropy)?;
    for _ in 1..if long { 4 } else { 2 } {
        enc.ec_enc_icdf(4, GAIN_PDF_DELTA, 8).map_err(entropy)?;
    }
    enc.ec_enc_icdf(
        usize::from(model.stage1),
        if bandwidth == Bandwidth::Wideband {
            silk::LSF_STAGE1_PDF_WB_INACTIVE
        } else {
            silk::LSF_STAGE1_PDF_NB_MB_INACTIVE
        },
        8,
    )
    .map_err(entropy)?;
    let codebooks = if bandwidth == Bandwidth::Wideband {
        &silk::LSF_CB_SELECT_WB[usize::from(model.stage1)][..]
    } else {
        &silk::LSF_CB_SELECT_NB[usize::from(model.stage1)][..]
    };
    for (&residual, &book) in model.residual.iter().zip(codebooks) {
        let pdf = SilkDecoder::get_lsf_stage2_pdf(book, bandwidth)?;
        enc.ec_enc_icdf((residual + 4) as usize, pdf, 8)
            .map_err(entropy)?;
    }
    if long {
        enc.ec_enc_icdf(4, &[243, 221, 192, 181, 0], 8)
            .map_err(entropy)?;
    }
    enc.ec_enc_icdf(0, &[192, 128, 64, 0], 8).map_err(entropy)?; // LCG seed.
    Ok(())
}

fn encode_pulses(enc: &mut RangeEncoder, pulses: &[i16]) -> Result<()> {
    // Rate level 8; escape to level 9 for coefficients requiring low bits.
    enc.ec_enc_icdf(8, silk::RATE_LEVEL_PDF_INACTIVE, 8)
        .map_err(entropy)?;
    let mut shells = Vec::new();
    for block in pulses.as_chunks::<16>().0 {
        let mut magnitudes: Vec<_> = block.iter().map(|x| x.unsigned_abs()).collect();
        let mut shifts = 0;
        while magnitudes.iter().sum::<u16>() > 16 {
            for value in &mut magnitudes {
                *value >>= 1;
            }
            shifts += 1;
        }
        let count = magnitudes.iter().sum::<u16>();
        if shifts == 0 {
            enc.ec_enc_icdf(usize::from(count), silk::PULSE_COUNT_PDF_LEVEL_8, 8)
                .map_err(entropy)?;
        } else {
            enc.ec_enc_icdf(17, silk::PULSE_COUNT_PDF_LEVEL_8, 8)
                .map_err(entropy)?;
            for _ in 1..shifts {
                enc.ec_enc_icdf(17, silk::PULSE_COUNT_PDF_LEVEL_9, 8)
                    .map_err(entropy)?;
            }
            enc.ec_enc_icdf(usize::from(count), silk::PULSE_COUNT_PDF_LEVEL_9, 8)
                .map_err(entropy)?;
        }
        shells.push((magnitudes, shifts, count));
    }
    for (magnitudes, _, _) in &shells {
        shell(enc, magnitudes)?;
    }
    for (block, (_, shifts, _)) in pulses.as_chunks::<16>().0.iter().zip(&shells) {
        for value in block {
            for shift in (0..*shifts).rev() {
                enc.ec_enc_icdf(
                    usize::from((value.unsigned_abs() >> shift) & 1),
                    silk::EXCITATION_LSB_PDF,
                    8,
                )
                .map_err(entropy)?;
            }
        }
    }
    let pdfs = [
        silk::SIGN_PDF_UNVOICED_LOW_0,
        silk::SIGN_PDF_UNVOICED_LOW_1,
        silk::SIGN_PDF_UNVOICED_LOW_2,
        silk::SIGN_PDF_UNVOICED_LOW_3,
        silk::SIGN_PDF_UNVOICED_LOW_4,
        silk::SIGN_PDF_UNVOICED_LOW_5,
        silk::SIGN_PDF_UNVOICED_LOW_6PLUS,
    ];
    for (block, (_, _, count)) in pulses.as_chunks::<16>().0.iter().zip(&shells) {
        for &value in block {
            if value != 0 {
                enc.ec_enc_icdf(
                    usize::from(value > 0),
                    pdfs[usize::from((*count).min(6))],
                    8,
                )
                .map_err(entropy)?;
            }
        }
    }
    Ok(())
}

fn shell(enc: &mut RangeEncoder, magnitudes: &[u16]) -> Result<()> {
    let total = magnitudes.iter().sum::<u16>();
    if total == 0 || magnitudes.len() == 1 {
        return Ok(());
    }
    let left = magnitudes[..magnitudes.len() / 2].iter().sum::<u16>();
    let pdf = silk::get_pulse_split_pdf(magnitudes.len(), total as u8)
        .ok_or_else(|| Error::Unsupported("invalid SILK shell pulse count".into()))?;
    enc.ec_enc_icdf(usize::from(left), pdf, 8)
        .map_err(entropy)?;
    shell(enc, &magnitudes[..magnitudes.len() / 2])?;
    shell(enc, &magnitudes[magnitudes.len() / 2..])
}
