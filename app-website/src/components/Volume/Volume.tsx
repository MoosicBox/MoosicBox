import { createEffect, createSignal, on, onCleanup, onMount } from 'solid-js';
import './Volume.css';
import { isServer } from 'solid-js/web';
import { playerState, setPlayerState, setVolume } from '~/services/player';

let mouseEnterListener: (event: MouseEvent) => void;
let mouseLeaveListener: (event: MouseEvent) => void;
let dragStartListener: (event: MouseEvent) => void;
let dragListener: (event: MouseEvent) => void;
let dragEndListener: (event: MouseEvent) => void;
let hideTimeout: NodeJS.Timeout | undefined;

let mouseY: number;

function eventToSeekPosition(element: HTMLElement): number {
    if (!element) return 0;

    const pos = element.getBoundingClientRect()!;
    const percentage = Math.min(
        1,
        Math.max(0, 1 - (mouseY - pos.top) / pos.height),
    );
    return Math.min(100, Math.max(0, Math.round(100 * percentage)));
}

export default function volumeRender() {
    let volumeContainerRef: HTMLImageElement;
    let volumeSliderInnerRef: HTMLImageElement;

    const [showVolume, setShowVolume] = createSignal(false);
    const [inside, setInside] = createSignal(false);
    const [sliderHeight, setSliderHeight] = createSignal(100);
    const [dragging, setDragging] = createSignal(false);
    const [applyDrag, setApplyDrag] = createSignal(false);

    function saveVolume(value: number) {
        if (isNaN(value)) {
            return;
        }
        let newVolume = value;
        if (value > 100) {
            newVolume = 100;
        } else if (value < 0) {
            newVolume = 0;
        }

        if (playerState.currentPlaybackSession?.volume !== newVolume / 100) {
            setVolume(newVolume / 100);
        }
    }

    createEffect(
        on(
            () => playerState.currentPlaybackSession?.volume ?? 1,
            (volume) => {
                const height = Math.round(
                    Math.max(0, Math.min(100, volume * 100)),
                );
                setSliderHeight(height);
            },
        ),
    );

    onMount(() => {
        if (isServer) {
            return;
        }

        function initiateClose() {
            hideTimeout = setTimeout(() => {
                setShowVolume(false);
                hideTimeout = undefined;
            }, 400);
        }

        mouseEnterListener = (_event: MouseEvent) => {
            setInside(true);
            if (hideTimeout) {
                clearTimeout(hideTimeout);
                hideTimeout = undefined;
            }
            setShowVolume(true);
        };

        mouseLeaveListener = (_event: MouseEvent) => {
            setInside(false);
            if (!dragging()) {
                initiateClose();
            }
        };

        dragStartListener = (event: MouseEvent) => {
            if (event.button === 0) {
                setDragging(true);
                setApplyDrag(true);
                mouseY = event.clientY;
                saveVolume(eventToSeekPosition(volumeSliderInnerRef));
            }
        };
        dragListener = (event: MouseEvent) => {
            if (!showVolume()) {
                return;
            }
            mouseY = event.clientY;
            if (dragging()) {
                event.preventDefault();
                if (!applyDrag()) return;
            } else {
                return;
            }
            saveVolume(eventToSeekPosition(volumeSliderInnerRef));
        };
        dragEndListener = (event: MouseEvent) => {
            if (event.button === 0 && dragging()) {
                setDragging(false);
                if (!inside() && showVolume()) {
                    initiateClose();
                }
                if (!applyDrag()) return;
                setApplyDrag(false);
                setPlayerState;
                event.preventDefault();
            }
        };

        volumeContainerRef.addEventListener('mouseenter', mouseEnterListener);
        volumeContainerRef.addEventListener('mouseleave', mouseLeaveListener);
        volumeSliderInnerRef.addEventListener('mousedown', dragStartListener);
        window.addEventListener('mousemove', dragListener);
        window.addEventListener('mouseup', dragEndListener);
    });

    onCleanup(() => {
        if (isServer) {
            return;
        }

        volumeContainerRef.removeEventListener(
            'mouseenter',
            mouseEnterListener,
        );
        volumeContainerRef.removeEventListener(
            'mouseleave',
            mouseLeaveListener,
        );
        volumeSliderInnerRef.removeEventListener(
            'mousedown',
            dragStartListener,
        );
        window.removeEventListener('mousemove', dragListener);
        window.removeEventListener('mouseup', dragEndListener);
    });

    return (
        <div class="volume-container" ref={volumeContainerRef!}>
            <img
                class="adjust-volume-icon"
                src="/img/audio-white.svg"
                alt="Adjust Volume"
                onClick={() => setShowVolume(!showVolume())}
            />
            <div
                class="volume-slider-container"
                style={{ display: showVolume() ? undefined : 'none' }}
            >
                <div class="volume-slider-inner" ref={volumeSliderInnerRef!}>
                    <div class="volume-slider-background"></div>
                    <div
                        class="volume-slider"
                        style={{
                            height: `${sliderHeight()}%`,
                        }}
                    ></div>
                    <div
                        class="volume-slider-top"
                        style={{ bottom: `${sliderHeight()}%` }}
                    ></div>
                </div>
            </div>
        </div>
    );
}
