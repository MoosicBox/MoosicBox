//! Bounded public multistream API for the test-only oracle.
use super::{Error, array, number};
use serde_json::{Value, json};
use std::ffi::{c_int, c_void};

unsafe extern "C" {
    fn opus_multistream_encoder_create(
        rate: i32,
        channels: c_int,
        streams: c_int,
        coupled: c_int,
        mapping: *const u8,
        application: c_int,
        error: *mut c_int,
    ) -> *mut c_void;
    fn opus_multistream_decoder_create(
        rate: i32,
        channels: c_int,
        streams: c_int,
        coupled: c_int,
        mapping: *const u8,
        error: *mut c_int,
    ) -> *mut c_void;
    fn opus_multistream_encode(
        state: *mut c_void,
        pcm: *const i16,
        frame_size: c_int,
        data: *mut u8,
        max_bytes: i32,
    ) -> c_int;
    fn opus_multistream_decode(
        state: *mut c_void,
        data: *const u8,
        len: i32,
        pcm: *mut i16,
        frame_size: c_int,
        fec: c_int,
    ) -> c_int;
    fn opus_multistream_encoder_ctl(state: *mut c_void, request: c_int, ...) -> c_int;
    fn opus_multistream_decoder_ctl(state: *mut c_void, request: c_int, ...) -> c_int;
    fn opus_multistream_encoder_destroy(state: *mut c_void);
    fn opus_multistream_decoder_destroy(state: *mut c_void);
}

struct State {
    ptr: *mut c_void,
    channels: usize,
    encoder: bool,
}
impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: a State owns precisely one successful create result.
        unsafe {
            if self.encoder {
                opus_multistream_encoder_destroy(self.ptr);
            } else {
                opus_multistream_decoder_destroy(self.ptr);
            }
        }
    }
}
#[derive(Default)]
pub struct Multistream {
    encoder: Option<State>,
    decoder: Option<State>,
}
fn check(code: i32) -> Result<usize, Error> {
    usize::try_from(code).map_err(|_| format!("multistream libopus error {code}").into())
}
impl Multistream {
    #[allow(clippy::too_many_lines)]
    pub fn request(&mut self, v: &Value) -> Result<Value, Error> {
        let op = v["op"].as_str().ok_or("missing op")?;
        match op {
            "multistream_encoder" | "multistream_decoder" => {
                let rate: i32 = number(v, "rate")?;
                let channels: u8 = number(v, "channels")?;
                let streams: u8 = number(v, "streams")?;
                let coupled: u8 = number(v, "coupled_streams")?;
                let mapping = array::<u8>(v, "mapping", 255)?;
                if channels == 0
                    || streams == 0
                    || coupled > streams
                    || u16::from(streams) + u16::from(coupled) > 255
                    || mapping.len() != usize::from(channels)
                {
                    return Err("invalid multistream layout".into());
                }
                let application = i32::try_from(v["application"].as_i64().unwrap_or(2049))?;
                let encoder = op == "multistream_encoder";
                let mut error = 0;
                // SAFETY: mapping has exactly channels entries; C validates mapping values and rate.
                let ptr = unsafe {
                    if encoder {
                        opus_multistream_encoder_create(
                            rate,
                            channels.into(),
                            streams.into(),
                            coupled.into(),
                            mapping.as_ptr(),
                            application,
                            &raw mut error,
                        )
                    } else {
                        opus_multistream_decoder_create(
                            rate,
                            channels.into(),
                            streams.into(),
                            coupled.into(),
                            mapping.as_ptr(),
                            &raw mut error,
                        )
                    }
                };
                check(error)?;
                if ptr.is_null() {
                    return Err("null multistream state".into());
                }
                let state = Some(State {
                    ptr,
                    channels: channels.into(),
                    encoder,
                });
                if encoder {
                    self.encoder = state;
                } else {
                    self.decoder = state;
                }
            }
            "multistream_encode" => {
                let state = self
                    .encoder
                    .as_mut()
                    .ok_or("create multistream encoder first")?;
                let size: usize = number(v, "frame_size")?;
                let pcm = array::<i16>(v, "pcm", 200_000)?;
                if size == 0 || size > 5760 || pcm.len() != size * state.channels {
                    return Err("invalid multistream PCM length".into());
                }
                let mut packet = vec![0; 65535];
                // SAFETY: validated PCM size and writable packet capacity.
                let count = check(unsafe {
                    opus_multistream_encode(
                        state.ptr,
                        pcm.as_ptr(),
                        i32::try_from(size)?,
                        packet.as_mut_ptr(),
                        65535,
                    )
                })?;
                packet.truncate(count);
                return Ok(json!({"packet":packet}));
            }
            "multistream_decode" => {
                let state = self
                    .decoder
                    .as_mut()
                    .ok_or("create multistream decoder first")?;
                let size: usize = number(v, "frame_size")?;
                if size == 0 || size > 5760 || size * state.channels > 100_000 {
                    return Err("invalid multistream output bound".into());
                }
                let packet = if v["packet"].is_null() {
                    Vec::new()
                } else {
                    array::<u8>(v, "packet", 65535)?
                };
                let mut pcm = vec![0; size * state.channels];
                // SAFETY: C receives exact packet length and frame capacity; null is explicit PLC.
                let count = check(unsafe {
                    opus_multistream_decode(
                        state.ptr,
                        if packet.is_empty() {
                            std::ptr::null()
                        } else {
                            packet.as_ptr()
                        },
                        i32::try_from(packet.len())?,
                        pcm.as_mut_ptr(),
                        i32::try_from(size)?,
                        i32::from(v["fec"].as_bool().unwrap_or(false)),
                    )
                })?;
                pcm.truncate(count * state.channels);
                return Ok(json!({"samples":count,"pcm":pcm}));
            }
            "multistream_reset" => {
                // SAFETY: owned live state, public no-argument reset CTL.
                check(unsafe {
                    match v["state"].as_str() {
                        Some("encoder") => opus_multistream_encoder_ctl(
                            self.encoder
                                .as_mut()
                                .ok_or("create multistream encoder first")?
                                .ptr,
                            4028,
                        ),
                        Some("decoder") => opus_multistream_decoder_ctl(
                            self.decoder
                                .as_mut()
                                .ok_or("create multistream decoder first")?
                                .ptr,
                            4028,
                        ),
                        _ => return Err("invalid multistream reset state".into()),
                    }
                })?;
            }
            "multistream_gain" => {
                let gain = i16::try_from(v["gain_q8"].as_i64().ok_or("invalid gain_q8")?)?;
                let state = self
                    .decoder
                    .as_mut()
                    .ok_or("create multistream decoder first")?;
                // SAFETY: owned decoder and public integer gain CTL.
                check(unsafe { opus_multistream_decoder_ctl(state.ptr, 4034, i32::from(gain)) })?;
            }
            _ => return Err("unknown multistream operation".into()),
        }
        Ok(json!({"ok":true}))
    }
}
