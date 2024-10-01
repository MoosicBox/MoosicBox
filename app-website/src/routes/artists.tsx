import ArtistPage from '~/components/pages/artists/artist-page';
import ArtistsPage from '~/components/pages/artists/artists-page';
import { getQueryParam } from '~/services/util';

export default function artistPage() {
    const artistId = getQueryParam('artistId');
    const tidalArtistId = getQueryParam('tidalArtistId');
    const qobuzArtistId = getQueryParam('qobuzArtistId');

    return (
        <>
            {artistId ? (
                <ArtistPage artistId={parseInt(artistId!)} />
            ) : tidalArtistId ? (
                <ArtistPage tidalArtistId={parseInt(tidalArtistId!)} />
            ) : qobuzArtistId ? (
                <ArtistPage qobuzArtistId={parseInt(qobuzArtistId!)} />
            ) : (
                <ArtistsPage />
            )}
        </>
    );
}
