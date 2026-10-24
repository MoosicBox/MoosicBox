//! Replay compatibility baseline captured with rand 0.8.5 on 64-bit targets.
#![cfg(target_pointer_width = "64")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

fn replay(seed: u64) -> Vec<u64> {
    let rng = switchy_random::Rng::from_seed(seed);
    let mut values = Vec::new();
    for _ in 0..128 {
        values.extend([
            rng.next_u64(),
            u64::from(rng.next_u32()),
            u64::from(rng.next_i32().cast_unsigned()),
            rng.gen_range(0_u64..1_000_003),
            u64::from(rng.gen_range(-1000_i32..=1000).cast_unsigned()),
            rng.random::<f64>().to_bits(),
            u64::from(rng.random::<f32>().to_bits()),
            u64::from(rng.gen_bool(0.37)),
            u64::from(rng.gen_ratio(2, 7)),
            rng.gen_range_dist(0.0_f64..1.0_f64, 2.5_f64).to_bits(),
        ]);
        let mut bytes = [0_u8; 16];
        rng.fill(&mut bytes);
        for chunk in bytes.as_chunks::<8>().0 {
            values.push(u64::from_le_bytes(*chunk));
        }
    }
    values
}

#[cfg(feature = "simulator")]
#[test]
fn historical_simulator_reset() {
    use switchy_random::simulator;
    const CHILD: &str = "SWITCHY_RANDOM_REPLAY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "historical_simulator_reset", "--nocapture"])
            .env(CHILD, "1")
            .env("SIMULATOR_SEED", "42")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    }
    assert_eq!(simulator::initial_seed(), 42);
    assert_eq!(simulator::seed(), 42);
    let expected = switchy_random::Rng::from_seed(42);
    simulator::reset_rng();
    let actual = simulator::rng();
    for _ in 0..128 {
        assert_eq!(actual.next_u64(), expected.next_u64());
    }
    simulator::reset_rng();
    assert_eq!(
        simulator::rng().next_u64(),
        switchy_random::Rng::from_seed(42).next_u64()
    );
    // The global generator advances the sequence used to seed simulation runs.
    let seed_source = switchy_random::Rng::from_seed(42);
    for _ in 0..8 {
        simulator::reset_seed();
        let seed = seed_source.next_u64();
        assert_eq!(simulator::seed(), seed);
        let expected = switchy_random::Rng::from_seed(seed);
        simulator::reset_rng();
        for _ in 0..16 {
            assert_eq!(simulator::rng().next_u64(), expected.next_u64());
        }
    }
}

#[test]
fn historical_seeded_replay() {
    for (seed, expected) in [
        (0, 17_869_191_804_123_392_245),
        (1, 11_588_557_473_108_275_971),
        (42, 14_309_709_537_806_800_332),
        (12345, 2_111_074_904_683_261_702),
        (u64::MAX, 12_379_848_376_455_795_659),
    ] {
        // FNV-1a over little-endian words makes the long mixed-operation trace
        // compact while checking exact float bits and random-consumption order.
        let digest = replay(seed)
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
            });
        assert_eq!(
            digest, expected,
            "rand 0.8.5 replay changed for seed {seed}"
        );
    }
}
