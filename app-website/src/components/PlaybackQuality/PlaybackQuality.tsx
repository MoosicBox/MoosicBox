import './playback-quality.css';
import { For, createComputed, createSignal } from 'solid-js';
import { Api } from '~/services/api';
import { playbackQuality, setPlaybackQuality } from '~/services/player';

export default function playbackQualityFunc() {
    const [quality, setQuality] = createSignal<Api.PlaybackQuality>(
        playbackQuality(),
    );

    createComputed(() => {
        setQuality(playbackQuality());
    });

    type AudioFormat = keyof typeof Api.AudioFormat;
    const formats: AudioFormat[] = Object.keys(
        Api.AudioFormat,
    ) as AudioFormat[];

    function selectFormat(format: AudioFormat) {
        setPlaybackQuality({ format: Api.AudioFormat[format] });
    }

    return (
        <div class="playback-quality">
            <div class="playback-quality-list">
                <For each={formats}>
                    {(format) => (
                        <div onClick={() => selectFormat(format)}>
                            {format}{' '}
                            {quality().format === Api.AudioFormat[format] && (
                                <span>(active)</span>
                            )}
                        </div>
                    )}
                </For>
            </div>
        </div>
    );
}
