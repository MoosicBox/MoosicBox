use moosicbox_opus_native::{Channels, Decoder, SampleRate};
use std::io::{Read, Write};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let rate = a[1].parse().unwrap();
    let channels = if a[2] == "1" {
        Channels::Mono
    } else {
        Channels::Stereo
    };
    let fec = a.get(3).is_some_and(|x| x == "fec");
    let mut d = Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
    let mut input = std::io::stdin();
    loop {
        let mut b = [0; 4];
        if input.read_exact(&mut b).is_err() {
            break;
        }
        let n = u32::from_le_bytes(b) as usize;
        let mut p = vec![0; n];
        input.read_exact(&mut p).unwrap();
        let mut out = vec![0; 5760 * channels as usize];
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            d.decode(if n == 0 { None } else { Some(&p) }, &mut out, fec)
        }));
        match r {
            Ok(Ok(n)) => {
                std::io::stdout()
                    .write_all(&(n as u32).to_le_bytes())
                    .unwrap();
                for s in &out[..n * channels as usize] {
                    std::io::stdout().write_all(&s.to_le_bytes()).unwrap();
                }
            }
            Ok(Err(e)) => {
                eprintln!("ERROR {e}");
                std::process::exit(2)
            }
            Err(_) => std::process::exit(3),
        }
    }
}
