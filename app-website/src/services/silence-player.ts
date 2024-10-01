import { QueryParams } from './util';
import { Api, getConnection } from './api';

let tryingToPlay = false;
let audio: HTMLAudioElement | undefined;

function initSilence() {
    console.debug('initSilence');
    const query = new QueryParams({
        duration: `${5}`,
        format: Api.AudioFormat.MP3,
    });

    const con = getConnection();
    const clientIdParam = con.clientId;
    const signatureToken = Api.signatureToken();

    if (con.clientId && signatureToken) {
        query.set('clientId', clientIdParam);
        query.set('signature', signatureToken);
    }
    if (con.staticToken) {
        query.set('authorization', con.staticToken);
    }

    const url = `${con.apiUrl}/files/silence?${query}`;

    audio = new Audio(url);

    // audio.addEventListener('timeupdate', (e) => {
    //     console.log(e.timeStamp, audio.currentTime);
    // });

    audio.loop = true;
    audio.play();
    audio.addEventListener('error', (e) => {
        console.error('Failed to start audio:', e.error);
        tryingToPlay = false;
        audio = undefined;
    });
}

export function isSilencePlaying(): boolean {
    return tryingToPlay || audio?.paused === false;
}

export function startSilence() {
    console.debug('startSilence');
    if (isSilencePlaying()) {
        console.debug('startSilence: already playing');
        return;
    }
    tryingToPlay = true;
    initSilence();
}

export function stopSilence() {
    console.debug('stopSilence');
    tryingToPlay = false;
    if (!isSilencePlaying()) {
        console.debug('stopSilence: already not playing');
        return;
    }
}
