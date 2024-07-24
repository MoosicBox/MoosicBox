use moosicbox_audio_output::{encoders::aac::AacEncoder, AudioOutputHandler};
use moosicbox_stream_utils::{ByteStream, ByteWriter};

use crate::play_file_path_str;

pub fn encode_aac_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_aac_spawn(path, writer);

    stream
}

pub fn encode_aac_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    moosicbox_task::spawn_blocking("audio_decoder: encode_aac", move || {
        encode_aac(path, writer)
    })
}

pub fn encode_aac<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                AacEncoder::with_writer(writer.clone()).open(spec, duration),
            ))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to aac: {err:?}");
    }
}
