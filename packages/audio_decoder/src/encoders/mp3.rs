use moosicbox_audio_outputs::{encoders::mp3::Mp3Encoder, AudioOutputHandler};
use moosicbox_stream_utils::{ByteStream, ByteWriter};

use crate::play_file_path_str;

pub fn encode_mp3_stream(path: String) -> ByteStream {
    let writer = ByteWriter::default();
    let stream = writer.stream();

    encode_mp3_spawn(path, writer);

    stream
}

pub fn encode_mp3_spawn<T: std::io::Write + Send + Sync + Clone + 'static>(
    path: String,
    writer: T,
) -> tokio::task::JoinHandle<()> {
    let path = path.clone();
    moosicbox_task::spawn_blocking("audio_decoder: encode_mp3", move || {
        encode_mp3(path, writer)
    })
}

pub fn encode_mp3<T: std::io::Write + Send + Sync + Clone + 'static>(path: String, writer: T) {
    let mut audio_output_handler =
        AudioOutputHandler::new().with_output(Box::new(move |spec, duration| {
            Ok(Box::new(
                Mp3Encoder::with_writer(writer.clone()).open(spec, duration),
            ))
        }));

    if let Err(err) = play_file_path_str(&path, &mut audio_output_handler, true, true, None, None) {
        log::error!("Failed to encode to mp3: {err:?}");
    }
}
