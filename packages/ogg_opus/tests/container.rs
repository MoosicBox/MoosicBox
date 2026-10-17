use moosicbox_ogg_opus::{OggOpusReader, OggOpusWriter, OpusHead, OpusTags, packet_duration};
use std::io::{self, Cursor, Read};
use std::process::{Command, Stdio};

fn stream(skip: u16, samples: u64, packets: usize) -> Vec<u8> {
    let mut writer = OggOpusWriter::new(
        Vec::new(),
        OpusHead::new(1, skip, 48000).unwrap(),
        &OpusTags::default(),
        73,
    )
    .unwrap();
    for _ in 0..packets {
        writer.write_packet(&[0xf8, 0xff, 0xfe]).unwrap();
    }
    writer.finish(samples).unwrap()
}

#[test]
fn variable_preskip_and_final_trim() {
    for skip in [0, 1, 120, 312, 960, 1500] {
        let bytes = stream(skip, 2700 - u64::from(skip), 3);
        let mut reader = OggOpusReader::new(Cursor::new(bytes)).unwrap();
        let mut samples = 0;
        let mut count = 0;
        while let Some(packet) = reader.next_packet().unwrap() {
            samples += u64::from(packet.audible_samples());
            count += 1;
        }
        assert_eq!(samples, 2700 - u64::from(skip));
        assert_eq!(count, 3);
    }
}

struct ShortRead(Cursor<Vec<u8>>);
impl Read for ShortRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = buffer.len().min(1);
        self.0.read(&mut buffer[..length])
    }
}
#[test]
fn short_reads_and_large_continued_tags() {
    let tags = OpusTags {
        vendor: vec![b'v'; 255 * 255],
        comments: vec![vec![b'x'; 255]],
    };
    let mut writer =
        OggOpusWriter::new(Vec::new(), OpusHead::new(2, 312, 44100).unwrap(), &tags, 1).unwrap();
    writer.write_packet(&[0xf8, 0xff, 0xfe]).unwrap();
    let bytes = writer.finish(600).unwrap();
    let mut reader = OggOpusReader::new(ShortRead(Cursor::new(bytes))).unwrap();
    assert_eq!(reader.tags(), &tags);
    assert_eq!(
        reader.next_packet().unwrap().unwrap().audible_samples(),
        600
    );
}

#[test]
fn packet_lacing_boundaries() {
    for length in [254, 255, 256, 65024, 65025, 65026, 130050] {
        let mut packet = vec![0; length];
        packet[0] = 0xf8;
        let mut writer = OggOpusWriter::new(
            Vec::new(),
            OpusHead::new(1, 0, 48000).unwrap(),
            &OpusTags::default(),
            8,
        )
        .unwrap();
        writer.write_packet(&packet).unwrap();
        let mut reader = OggOpusReader::new(Cursor::new(writer.finish(960).unwrap())).unwrap();
        assert_eq!(reader.next_packet().unwrap().unwrap().data, packet);
        assert!(reader.next_packet().unwrap().is_none());
    }
}

#[test]
fn rejects_corruption_and_truncation() {
    let bytes = stream(312, 1500, 2);
    for length in 0..bytes.len() {
        let result = OggOpusReader::new(Cursor::new(bytes[..length].to_vec()));
        if let Ok(mut reader) = result {
            let mut failed = false;
            loop {
                match reader.next_packet() {
                    Err(_) => {
                        failed = true;
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(_)) => {}
                }
            }
            assert!(failed, "accepted truncation at {length}");
        }
    }
    for position in 0..bytes.len() {
        let mut damaged = bytes.clone();
        damaged[position] ^= 1;
        let result = OggOpusReader::new(Cursor::new(damaged));
        if let Ok(mut reader) = result {
            let mut failed = false;
            loop {
                match reader.next_packet() {
                    Err(_) => {
                        failed = true;
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(_)) => {}
                }
            }
            assert!(failed, "accepted corruption at {position}");
        }
    }
}

#[test]
fn seek_preroll_and_clamping() {
    let mut reader = OggOpusReader::new(Cursor::new(stream(312, 9000, 10))).unwrap();
    let seek = reader.seek_to(7000).unwrap();
    assert_eq!(seek.sample, 7000);
    assert_eq!(seek.discard_until, 7312);
    let packet = reader.next_packet().unwrap().unwrap();
    assert!(packet.decoded_start <= 7312 - 3840);
    let seek = reader.seek_to(u64::MAX).unwrap();
    assert_eq!(seek.sample, 9000);
    assert_eq!(seek.discard_until, 9312);
    let seek = reader.seek_to(0).unwrap();
    assert_eq!(seek.discard_until, 312);
    assert_eq!(reader.next_packet().unwrap().unwrap().trim_start, 312);
}

#[test]
fn malformed_headers_and_duration() {
    assert!(OpusHead::new(3, 0, 48000).is_err());
    assert!(OpusHead::parse(b"OpusHead").is_err());
    assert!(OpusTags::parse(b"OpusTags\xff\xff\xff\xff").is_err());
    assert!(packet_duration(&[]).is_err());
    assert!(packet_duration(&[3]).is_err());
    assert!(packet_duration(&[3, 0]).is_err());
    assert!(packet_duration(&[3, 63]).is_err());
}

// Independent interoperability test, explicitly ignored when FFmpeg is unavailable.
#[test]
#[ignore = "requires ffmpeg with libopus; run explicitly for independent interop"]
fn ffmpeg_decodes_exact_audible_length() {
    use std::io::Write;
    for skip in [0_u16, 1, 120, 312, 960, 1500] {
        let packets = usize::try_from((2457 + u64::from(skip)).div_ceil(960)).unwrap();
        let bytes = stream(skip, 2457, packets);
        let mut child = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-i",
                "pipe:0",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len() / 4, 2457, "pre_skip {skip}");
    }
}

#[test]
#[ignore = "requires ffmpeg with libopus; run explicitly for independent interop"]
fn reads_independent_ffmpeg_stream() {
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=0.0511875",
            "-c:a",
            "libopus",
            "-f",
            "opus",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut reader = OggOpusReader::new(Cursor::new(output.stdout)).unwrap();
    let mut total = 0_u64;
    while let Some(packet) = reader.next_packet().unwrap() {
        total += u64::from(packet.audible_samples());
    }
    assert_eq!(total, 2457);
}
