//! Randomness from the OS, drawn from the simulation run instead.
//!
//! `getrandom` (0.3 and 0.4) is where `uuid`, `rand`'s OS generator, hashers' random seeds and
//! most other crates get randomness from the operating system. Built with
//! `--cfg getrandom_backend="custom"` (a simulation workspace's `.cargo/config.toml`, never a
//! real build), it calls the function below instead, which fills the buffer from the current
//! run's `getrandom` stream ([`switchy_simulation::stream`]): the same seed gives the same
//! bytes, in any process, and the stream's state is part of the run's digest.
//!
//! Outside a run ([`switchy_simulation::in_run`]) nothing seeds the bytes, so a call panics,
//! naming what to use instead.
//!
//! `getrandom` 0.2 cannot be redirected on systems it supports natively (its custom backend
//! is a fallback for other targets only); its users (`rand` 0.8's `OsRng` and `thread_rng`)
//! are forbidden by the simulations' determinism check instead.

/// Fill `dest` from the current run's `getrandom` stream.
///
/// # Panics
///
/// * Outside a simulation run
pub fn fill(dest: &mut [u8]) {
    assert!(
        switchy_simulation::in_run(),
        "operating-system randomness (getrandom) was requested outside a simulation run, where \
         no seed controls it: draw from switchy_random (or derive the value from the run) \
         instead, or move the call into the run"
    );
    let stream = switchy_simulation::stream("getrandom");
    let mut rng = stream
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for chunk in dest.chunks_mut(8) {
        let bytes = rng.next_u64().to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    drop(rng);
}

/// `getrandom` 0.3's and 0.4's custom backend (both call this one symbol, with the same
/// error type).
///
/// # Safety
///
/// `dest` must be valid for writes of `len` bytes, as `getrandom` guarantees.
#[cfg(getrandom_backend = "custom")]
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom_04::Error> {
    let mut bytes = vec![0; len];
    fill(&mut bytes);
    // SAFETY: the caller guarantees `dest` is writable for `len` bytes; `bytes` has `len`.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), dest, len) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::fill;

    fn draw(seed: u64, len: usize) -> Vec<u8> {
        let run = switchy_simulation::Context::new(seed);
        run.begin_run();
        switchy_simulation::with_context(run, || {
            let mut bytes = vec![0; len];
            fill(&mut bytes);
            bytes
        })
    }

    #[test]
    fn a_runs_bytes_follow_its_seed() {
        assert_eq!(draw(1, 13), draw(1, 13));
        assert_ne!(draw(1, 13), draw(2, 13));
        assert_eq!(draw(1, 13).len(), 13);
    }

    #[test]
    fn draws_come_from_the_runs_getrandom_stream() {
        let run = switchy_simulation::Context::new(5);
        run.begin_run();
        switchy_simulation::with_context(Arc::clone(&run), || {
            let mut bytes = [0; 8];
            fill(&mut bytes);
            let expected = switchy_simulation::Context::new(5)
                .stream("getrandom")
                .lock()
                .unwrap()
                .next_u64();
            assert_eq!(bytes, expected.to_le_bytes());
        });
    }

    #[test]
    fn outside_a_run_it_panics() {
        let result = std::thread::spawn(|| fill(&mut [0; 4])).join();
        assert!(result.is_err());
    }

    /// With the custom backend (`RUSTFLAGS='--cfg getrandom_backend="custom"'`), `getrandom`
    /// itself draws from the run.
    #[cfg(getrandom_backend = "custom")]
    #[test]
    fn getrandom_draws_from_the_run() {
        let via_getrandom = {
            let run = switchy_simulation::Context::new(9);
            run.begin_run();
            switchy_simulation::with_context(run, || {
                let mut bytes = [0; 16];
                getrandom_04::fill(&mut bytes).unwrap();
                bytes
            })
        };
        assert_eq!(via_getrandom.to_vec(), draw(9, 16));
    }
}
