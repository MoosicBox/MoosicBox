import AlbumPage from '~/components/pages/albums/album-page';
import AlbumsPage from '~/components/pages/albums/albums-page';
import { getQueryParam } from '~/services/util';

export default function albumPage() {
    const albumId = getQueryParam('albumId');
    const tidalAlbumId = getQueryParam('tidalAlbumId');
    const qobuzAlbumId = getQueryParam('qobuzAlbumId');

    return (
        <>
            {albumId ? (
                <AlbumPage albumId={parseInt(albumId!)} />
            ) : tidalAlbumId ? (
                <AlbumPage tidalAlbumId={parseInt(tidalAlbumId!)} />
            ) : qobuzAlbumId ? (
                <AlbumPage qobuzAlbumId={qobuzAlbumId!} />
            ) : (
                <AlbumsPage />
            )}
        </>
    );
}
