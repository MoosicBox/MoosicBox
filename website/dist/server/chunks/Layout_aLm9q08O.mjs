import { c as createComponent, r as renderTemplate, d as renderComponent, b as createAstro, m as maybeRenderHead, a as addAttribute, e as renderHead, f as renderSlot } from './astro/server_BHkv7Nwt.mjs';
/* empty css                          */
import { isServer, ssr, ssrHydrationKey, escape, ssrAttribute, createComponent as createComponent$1 } from 'solid-js/web';
import { createSignal, onMount, onCleanup, createComputed, createEffect, on, For, Show } from 'solid-js';
/* empty css                          */
import { makePersisted } from '@solid-primitives/storage';
import { atom } from 'nanostores';
import { persistentAtom } from '@nanostores/persistent';
import { createStore, produce } from 'solid-js/store';

const $$Astro$1 = createAstro();
const $$Aside = createComponent(($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro$1, $$props, $$slots);
  Astro2.self = $$Aside;
  const search = Object.fromEntries(new URLSearchParams(Astro2.url.searchParams));
  return renderTemplate`${renderComponent($$result, "navigation-sidebar", "navigation-sidebar", { "data-turbo-permanent": true, "id": "navigation-sidebar" }, { "default": () => renderTemplate` ${maybeRenderHead()}<aside${addAttribute(`navigation-bar-container${search.navToggled === "true" ? " toggled" : " default"}`, "class")}> <div class="navigation-bar"> <div class="navigation-bar-header"> <a href="/" class="navigation-bar-header-home-link"> <img class="navigation-bar-header-home-link-logo-icon" src="/img/icon128.png"> <h1 class="navigation-bar-header-home-link-text">
MoosicBox
</h1> </a> <div class="navigation-bar-header-action-links"> <a class="settings-link" href="/settings" aria-describedby="settings-link-description"> <img class="settings-gear-icon" src="/img/settings-gear-white.svg" alt="View MoosicBox Settings"> </a> <div id="settings-link-description" style="display:none">
View MoosicBox Settings
</div> <button type="button" class="toggle-expand-button" aria-describedby="toggle-expand-button-link-description"> <img class="collapse-navigation-bar" src="/img/chevron-left-white.svg" alt="Collapse navigation"> <img class="expand-navigation-bar" src="/img/chevron-right-white.svg" alt="Expand navigation"> </button> <div id="toggle-expand-button-link-description" style="display:none">
Toggle navigation bar collapsed
</div> </div> </div> <ul> <li> <a href="/">Home</a> </li> <li> <a href="/downloads">Downloads</a> </li> </ul> <h1 class="my-collection-header">My Collection</h1> <ul> <li> <a href="/albums">Albums</a> </li> <li> <a href="/artists">Artists</a> </li> </ul> </div> <div class="mobile-toggle-expand-button"> <img class="mobile-expand-navigation-bar" src="/img/chevron-left.svg"> </div> </aside> ` })} `;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/layouts/Aside.astro", void 0);

class ClientAtom {
  _atom;
  _initial;
  _prevValue;
  _name;
  _listeners = [];
  constructor(initial, name) {
    this._initial = initial;
    this._name = name;
    if (name) {
      this._atom = persistentAtom(name, initial, {
        encode: JSON.stringify,
        decode: JSON.parse
      });
    } else {
      this._atom = atom(initial);
    }
    this._prevValue = this.get();
  }
  get name() {
    return this._name;
  }
  get initial() {
    return this._initial;
  }
  get() {
    return this._atom.get();
  }
  set(value) {
    this._atom.set(value);
    this._prevValue = value;
  }
  listen(listener) {
    const mapping = {
      baseListener: (v) => listener(v, this._prevValue),
      listener
    };
    this._atom.listen(mapping.baseListener);
    this._listeners.push(mapping);
  }
  off(listener) {
    const index = this._listeners.findIndex((x) => x.listener === listener);
    if (index !== -1) {
      this._listeners.splice(index, 1);
      this._atom.off();
      this._listeners.forEach((mapping) => {
        this._atom.listen(mapping.baseListener);
      });
    }
  }
}
function clientAtom(initial, name) {
  return new ClientAtom(initial, name);
}
function clientSignal(atom2) {
  let init = true;
  const [get, set] = createSignal(atom2.get(), {
    equals(a, b) {
      if (init) {
        init = false;
        return false;
      }
      return a === b;
    }
  });
  const listener = (value) => {
    set(value);
  };
  onMount(() => {
    set(atom2.get());
    atom2.listen(listener);
  });
  onCleanup(() => {
    atom2.off(listener);
  });
  return [
    () => {
      const wasInit = init;
      const value = get();
      if (wasInit) {
        return atom2.initial;
      } else {
        return value;
      }
    },
    (value) => {
      atom2.set(value);
    }
  ];
}
function createListener() {
  let listeners = [];
  function on(callback) {
    listeners.push(callback);
    return callback;
  }
  function onFirst(callback) {
    listeners.unshift(callback);
    return callback;
  }
  function off(callback) {
    listeners = listeners.filter((c) => c !== callback);
  }
  const trigger = (...args) => {
    for (const listener of listeners) {
      if (listener(...args) === false) {
        break;
      }
    }
  };
  return { on, onFirst, off, listeners, trigger };
}
function orderedEntries(value, order) {
  const updates = Object.entries(value);
  updates.sort(([key1], [key2]) => {
    let first = order.indexOf(key1);
    let second = order.indexOf(key2);
    first = first === -1 ? order.length : first;
    second = second === -1 ? order.length : second;
    return first - second;
  });
  return updates;
}
class QueryParams {
  params;
  constructor(init) {
    this.params = [];
    if (typeof init === "string") {
      if (init[0] === "?") {
        init = init.substring(1);
      }
      if (init.trim().length > 0) {
        init.split("&").map((pair) => pair.split("=")).forEach(([key, value]) => {
          this.params.push([key, value]);
        });
      }
    } else if (init instanceof QueryParams) {
      this.params.push(...init.params);
    } else if (init) {
      Object.entries(init).forEach(([key, value]) => {
        if (typeof value === "undefined") return;
        this.params.push([key, value]);
      });
    }
  }
  get size() {
    return this.params.length;
  }
  has(key) {
    return !!this.params.find(([k, _value]) => k === key);
  }
  get(key) {
    const value = this.params.find(([k, _value]) => k === key);
    if (value) {
      return value[1];
    }
    return void 0;
  }
  set(key, value) {
    const existing = this.params.find(([k, _value]) => k === key);
    if (existing) {
      existing[1] = value;
    } else {
      this.params.push([key, value]);
    }
  }
  delete(key) {
    this.params = this.params.filter(([k, _value]) => k !== key);
  }
  forEach(func) {
    this.params.forEach(([key, value]) => func(key, value));
  }
  toString() {
    return `${this.params.map(
      ([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`
    ).join("&")}`;
  }
}
function circularStringify(obj) {
  const getCircularReplacer = () => {
    const seen = /* @__PURE__ */ new WeakSet();
    return (_key, value) => {
      if (typeof value === "object" && value !== null) {
        if (seen.has(value)) {
          return "[[circular]]";
        }
        seen.add(value);
      }
      return value;
    };
  };
  return JSON.stringify(obj, getCircularReplacer());
}
function objToStr(obj) {
  if (typeof obj === "string") {
    return obj;
  } else if (typeof obj === "undefined") {
    return "undefined";
  } else if (obj === null) {
    return "null";
  } else if (typeof obj === "object") {
    return circularStringify(obj);
  } else {
    return obj.toString();
  }
}
function throwExpression(errorMessage) {
  throw new Error(errorMessage);
}
function deepEqual(x, y) {
  if (x === y) {
    return true;
  } else if (typeof x == "object" && x != null && typeof y == "object" && y != null) {
    if (Object.keys(x).length != Object.keys(y).length) return false;
    for (const prop in x) {
      if (y.hasOwnProperty(prop)) {
        if (!deepEqual(x[prop], y[prop])) return false;
      } else return false;
    }
    return true;
  } else return false;
}

function trackId(track) {
  if (!track) return void 0;
  if (typeof track === "number" || typeof track === "string") return track;
  return "trackId" in track ? track.trackId : "id" in track ? track.id : void 0;
}
function toSessionPlaylistTrack(track) {
  if (track.type === "LIBRARY") {
    return {
      id: `${track.trackId}`,
      type: track.type,
      data: JSON.stringify(track)
    };
  } else {
    return {
      id: `${track.id}`,
      type: track.type,
      data: JSON.stringify(track)
    };
  }
}
var Api;
((Api2) => {
  const onSignatureTokenUpdatedListeners = createListener();
  Api2.onSignatureTokenUpdated = onSignatureTokenUpdatedListeners.on;
  Api2.offSignatureTokenUpdated = onSignatureTokenUpdatedListeners.off;
  const [_signatureToken, _setSignatureToken] = makePersisted(
    createSignal("api.v2.signatureToken"),
    {
      name: "signatureToken"
    }
  );
  function signatureToken() {
    return _signatureToken();
  }
  Api2.signatureToken = signatureToken;
  function setSignatureToken(url) {
    if (url === _signatureToken()) {
      return;
    }
    _setSignatureToken(url);
    onSignatureTokenUpdatedListeners.trigger(url);
  }
  Api2.setSignatureToken = setSignatureToken;
  ((PlayerType2) => {
    PlayerType2["HOWLER"] = "HOWLER";
  })(Api2.PlayerType || (Api2.PlayerType = {}));
  ((TrackSource2) => {
    TrackSource2["LOCAL"] = "LOCAL";
    TrackSource2["TIDAL"] = "TIDAL";
    TrackSource2["QOBUZ"] = "QOBUZ";
    TrackSource2["YT"] = "YT";
  })(Api2.TrackSource || (Api2.TrackSource = {}));
  Api2.AudioFormat = {
    AAC: "AAC",
    FLAC: "FLAC",
    MP3: "MP3",
    OPUS: "OPUS",
    SOURCE: "SOURCE"
  };
  function getPath(path) {
    path = path[0] === "/" ? path.substring(1) : path;
    const containsQuery = path.includes("?");
    const params = [];
    const con = getConnection();
    const clientId = con.clientId;
    if (con.clientId) {
      params.push(`clientId=${encodeURIComponent(clientId)}`);
    }
    const signatureToken2 = Api2.signatureToken();
    if (signatureToken2) {
      params.push(`signature=${encodeURIComponent(signatureToken2)}`);
    }
    if (con.staticToken) {
      params.push(`authorization=${encodeURIComponent(con.staticToken)}`);
    }
    const query = `${containsQuery ? "&" : "?"}${params.join("&")}`;
    return `${con.apiUrl}/${path}${query}`;
  }
  Api2.getPath = getPath;
  Api2.TrackAudioQuality = {
    Low: "LOW",
    // MP3 320
    FlacLossless: "FLAC_LOSSLESS",
    // FLAC 16 bit 44.1kHz
    FlacHiRes: "FLAC_HI_RES",
    // FLAC 24 bit <= 96kHz
    FlacHighestRes: "FLAC_HIGHEST_RES"
    // FLAC 24 bit > 96kHz <= 192kHz
  };
})(Api || (Api = {}));
const connections = clientAtom([], "api.v2.connections");
const $connections = () => connections.get();
const connection = clientAtom(
  $connections()[0] ?? null,
  "api.v2.connection"
);
const $connection = () => connection.get();
let connectionId$1 = 1;
$connections()?.forEach((x) => {
  if (x.id >= connectionId$1) {
    connectionId$1 = x.id + 1;
  }
});
function getConnection() {
  return $connection() ?? throwExpression("No connection selected");
}
async function getArtist(artistId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    artistId: `${artistId}`
  });
  return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
function getAlbumArtwork(album, width, height) {
  if (!album) return "/img/album.svg";
  const albumType = album.type;
  const query = new QueryParams({
    source: albumType,
    artistId: album.artistId?.toString()
  });
  switch (albumType) {
    case "LIBRARY":
      if (album.containsCover) {
        return Api.getPath(
          `files/albums/${album.albumId}/${width}x${height}?${query}`
        );
      }
      break;
    case "TIDAL":
      if (album.containsCover) {
        if ("albumId" in album) {
          return Api.getPath(
            `files/albums/${album.albumId}/${width}x${height}?${query}`
          );
        } else if ("id" in album) {
          return Api.getPath(
            `files/albums/${album.id}/${width}x${height}?${query}`
          );
        }
      }
      break;
    case "QOBUZ":
      if (album.containsCover) {
        if ("albumId" in album) {
          return Api.getPath(
            `files/albums/${album.albumId}/${width}x${height}?${query}`
          );
        } else if ("id" in album) {
          return Api.getPath(
            `files/albums/${album.id}/${width}x${height}?${query}`
          );
        }
      }
      break;
    case "YT":
      if (album.containsCover) {
        if ("albumId" in album) {
          return Api.getPath(
            `files/albums/${album.albumId}/${width}x${height}?${query}`
          );
        } else if ("id" in album) {
          return Api.getPath(
            `files/albums/${album.id}/${width}x${height}?${query}`
          );
        }
      }
      break;
  }
  return "/img/album.svg";
}
function getAlbumSourceArtwork(album) {
  if (!album) return "/img/album.svg";
  const albumType = album.type;
  const query = new QueryParams({
    source: albumType,
    artistId: album.artistId.toString()
  });
  switch (albumType) {
    case "LIBRARY":
      if (album.containsCover) {
        return Api.getPath(
          `files/albums/${album.albumId}/source?${query}`
        );
      }
      break;
    case "TIDAL":
      if (album.containsCover) {
        if ("albumId" in album) {
          return Api.getPath(
            `files/albums/${album.albumId}/source?${query}`
          );
        } else if ("id" in album) {
          return Api.getPath(
            `files/albums/${album.id}/source?${query}`
          );
        }
      }
      break;
    case "QOBUZ":
      if (album.containsCover) {
        if ("albumId" in album) {
          return Api.getPath(
            `files/albums/${album.albumId}/source?${query}`
          );
        } else if ("id" in album) {
          return Api.getPath(
            `files/albums/${album.id}/source?${query}`
          );
        }
      }
      break;
    case "YT":
      if (album.containsCover) {
        if ("albumId" in album) {
          return Api.getPath(
            `files/albums/${album.albumId}/source?${query}`
          );
        } else if ("id" in album) {
          return Api.getPath(
            `files/albums/${album.id}/source?${query}`
          );
        }
      }
      break;
  }
  return "/img/album.svg";
}
async function getAlbum(id, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: `${id}`
  });
  return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getAlbums(albumsRequest = void 0, signal) {
  const con = getConnection();
  const query = new QueryParams({
    artistId: albumsRequest?.artistId?.toString(),
    tidalArtistId: albumsRequest?.tidalArtistId?.toString(),
    qobuzArtistId: albumsRequest?.qobuzArtistId?.toString(),
    offset: `${albumsRequest?.offset ?? 0}`,
    limit: `${albumsRequest?.limit ?? 100}`
  });
  if (albumsRequest?.sources)
    query.set("sources", albumsRequest.sources.join(","));
  if (albumsRequest?.sort) query.set("sort", albumsRequest.sort);
  if (albumsRequest?.filters?.search)
    query.set("search", albumsRequest.filters.search);
  return await requestJson(`${con.apiUrl}/menu/albums?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getAllAlbums(albumsRequest = void 0, onAlbums, signal) {
  let offset = albumsRequest?.offset ?? 0;
  let limit = albumsRequest?.limit ?? 100;
  albumsRequest = albumsRequest ?? { offset, limit };
  const page = await getAlbums(albumsRequest, signal);
  let items = page.items;
  onAlbums?.(page.items, items, 0);
  if (signal?.aborted || !page.hasMore) return items;
  offset = limit;
  limit = Math.min(Math.max(100, Math.ceil((page.total - limit) / 6)), 1e3);
  const requests = [];
  do {
    requests.push({ ...albumsRequest, offset, limit });
    offset += limit;
  } while (offset < page.total);
  const output = [items, ...requests.map(() => [])];
  await Promise.all(
    requests.map(async (request, i) => {
      const page2 = await getAlbums(request, signal);
      output[i + 1] = page2.items;
      items = output.flat();
      onAlbums?.(page2.items, items, i + 1);
      return page2;
    })
  );
  return items;
}
function getArtistCover(artist, width, height) {
  if (!artist) return "/img/album.svg";
  const artistType = artist.type;
  const query = new QueryParams({
    source: artistType
  });
  switch (artistType) {
    case "LIBRARY":
      if (artist.containsCover) {
        return Api.getPath(
          `files/artists/${artist.artistId}/${width}x${height}?${query}`
        );
      }
      break;
    case "TIDAL":
      if (artist.containsCover) {
        if ("artistId" in artist) {
          return Api.getPath(
            `files/artists/${artist.artistId}/${width}x${height}?${query}`
          );
        } else if ("id" in artist) {
          return Api.getPath(
            `files/artists/${artist.id}/${width}x${height}?${query}`
          );
        }
      }
      break;
    case "QOBUZ":
      if (artist.containsCover) {
        if ("artistId" in artist) {
          return Api.getPath(
            `files/artists/${artist.artistId}/${width}x${height}?${query}`
          );
        } else if ("id" in artist) {
          return Api.getPath(
            `files/artists/${artist.id}/${width}x${height}?${query}`
          );
        }
      }
      break;
    case "YT":
      if (artist.containsCover) {
        if ("artistId" in artist) {
          return Api.getPath(
            `files/artists/${artist.artistId}/${width}x${height}?${query}`
          );
        } else if ("id" in artist) {
          return Api.getPath(
            `files/artists/${artist.id}/${width}x${height}?${query}`
          );
        }
      }
      break;
  }
  return "/img/album.svg";
}
function getArtistSourceCover(artist) {
  if (!artist) return "/img/album.svg";
  const artistType = artist.type;
  const query = new QueryParams({
    source: artistType
  });
  switch (artistType) {
    case "LIBRARY":
      if (artist.containsCover) {
        return Api.getPath(
          `files/artists/${artist.artistId}/source?${query}`
        );
      }
      break;
    case "TIDAL":
      if (artist.containsCover) {
        if ("artistId" in artist) {
          return Api.getPath(
            `files/artists/${artist.artistId}/source?${query}`
          );
        } else if ("id" in artist) {
          return Api.getPath(
            `files/artists/${artist.id}/source?${query}`
          );
        }
      }
      break;
    case "QOBUZ":
      if (artist.containsCover) {
        if ("artistId" in artist) {
          return Api.getPath(
            `files/artists/${artist.artistId}/source?${query}`
          );
        } else if ("id" in artist) {
          return Api.getPath(
            `files/artists/${artist.id}/source?${query}`
          );
        }
      }
      break;
    case "YT":
      if (artist.containsCover) {
        if ("artistId" in artist) {
          return Api.getPath(
            `files/artists/${artist.artistId}/source?${query}`
          );
        } else if ("id" in artist) {
          return Api.getPath(
            `files/artists/${artist.id}/source?${query}`
          );
        }
      }
      break;
  }
  return "/img/album.svg";
}
async function getAlbumTracks(albumId, signal) {
  const con = getConnection();
  return await requestJson(
    `${con.apiUrl}/menu/album/tracks?albumId=${albumId}`,
    {
      method: "GET",
      credentials: "include",
      signal: signal ?? null
    }
  );
}
async function getAlbumVersions(albumId, signal) {
  const con = getConnection();
  return await requestJson(
    `${con.apiUrl}/menu/album/versions?albumId=${albumId}`,
    {
      method: "GET",
      credentials: "include",
      signal: signal ?? null
    }
  );
}
async function getTracks(trackIds, signal) {
  const con = getConnection();
  return await requestJson(
    `${con.apiUrl}/menu/tracks?trackIds=${trackIds.join(",")}`,
    {
      method: "GET",
      credentials: "include",
      signal: signal ?? null
    }
  );
}
async function getArtists(artistsRequest = void 0, signal) {
  const con = getConnection();
  const query = new QueryParams();
  if (artistsRequest?.sources)
    query.set("sources", artistsRequest.sources.join(","));
  if (artistsRequest?.sort) query.set("sort", artistsRequest.sort);
  if (artistsRequest?.filters?.search)
    query.set("search", artistsRequest.filters.search);
  return await requestJson(`${con.apiUrl}/menu/artists?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function fetchSignatureToken(signal) {
  const con = getConnection();
  const { token } = await requestJson(
    `${con.apiUrl}/auth/signature-token`,
    {
      credentials: "include",
      method: "POST",
      signal: signal ?? null
    }
  );
  return token;
}
const [nonTunnelApis, setNonTunnelApis] = makePersisted(
  createSignal([]),
  {
    name: "nonTunnelApis"
  }
);
async function validateSignatureTokenAndClient(signature, signal) {
  const con = getConnection();
  const apis = nonTunnelApis();
  if (apis.includes(con.apiUrl)) {
    return { notFound: true };
  }
  try {
    const { valid } = await requestJson(
      `${con.apiUrl}/auth/validate-signature-token?signature=${signature}`,
      {
        credentials: "include",
        method: "POST",
        signal: signal ?? null
      }
    );
    return { valid: !!valid };
  } catch (e) {
    if (e instanceof RequestError) {
      if (e.response.status === 404) {
        setNonTunnelApis([...apis, con.apiUrl]);
        return { notFound: true };
      }
    }
    return { valid: false };
  }
}
async function refetchSignatureToken() {
  console.debug("Refetching signature token");
  const token = await api.fetchSignatureToken();
  if (token) {
    Api.setSignatureToken(token);
  } else {
    console.error("Failed to fetch signature token");
  }
}
async function validateSignatureToken() {
  const con = getConnection();
  if (!con.token) return;
  const existing = Api.signatureToken();
  if (!existing) {
    await api.refetchSignatureToken();
    return;
  }
  const { valid, notFound } = await api.validateSignatureTokenAndClient(existing);
  if (notFound) {
    console.debug("Not hitting tunnel server");
    return;
  }
  if (!valid) {
    await api.refetchSignatureToken();
  }
}
async function magicToken(magicToken2, signal) {
  const con = getConnection();
  try {
    return await requestJson(
      `${con.apiUrl}/auth/magic-token?magicToken=${magicToken2}`,
      {
        credentials: "include",
        signal: signal ?? null
      }
    );
  } catch {
    return false;
  }
}
async function globalSearch(query, offset, limit, signal) {
  const con = getConnection();
  const queryParams = new QueryParams({
    query,
    offset: offset?.toString() ?? void 0,
    limit: limit?.toString() ?? void 0
  });
  return await requestJson(
    `${con.apiUrl}/search/global-search?${queryParams.toString()}`,
    {
      credentials: "include",
      signal: signal ?? null
    }
  );
}
async function searchExternalMusicApi(query, api2, offset, limit, signal) {
  const con = getConnection();
  const queryParams = new QueryParams({
    query,
    offset: offset?.toString() ?? void 0,
    limit: limit?.toString() ?? void 0
  });
  return await requestJson(
    `${con.apiUrl}/${api2}/search?${queryParams.toString()}`,
    {
      credentials: "include",
      signal: signal ?? null
    }
  );
}
async function searchAll(query, offset, limit, onResults, signal) {
  const allResults = [];
  await Promise.all([
    (async () => {
      const results = (await globalSearch(query, offset, limit, signal)).results;
      allResults.push(...allResults);
      onResults?.(results, allResults, "LIBRARY");
    })(),
    (async () => {
      const results = (await searchExternalMusicApi(
        query,
        "tidal",
        offset,
        limit,
        signal
      )).results;
      allResults.push(...allResults);
      onResults?.(results, allResults, "TIDAL");
    })(),
    (async () => {
      const results = (await searchExternalMusicApi(
        query,
        "qobuz",
        offset,
        limit,
        signal
      )).results;
      allResults.push(...allResults);
      onResults?.(results, allResults, "QOBUZ");
    })(),
    (async () => {
      const results = (await searchExternalMusicApi(query, "yt", offset, limit, signal)).results;
      allResults.push(...allResults);
      onResults?.(results, allResults, "YT");
    })()
  ]);
  return allResults;
}
async function getArtistFromTidalArtistId(tidalArtistId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    tidalArtistId: `${tidalArtistId}`
  });
  return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getArtistFromQobuzArtistId(qobuzArtistId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    qobuzArtistId: `${qobuzArtistId}`
  });
  return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getArtistFromTidalAlbumId(tidalAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    tidalAlbumId: `${tidalAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/menu/artist?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getTidalArtist(tidalArtistId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    artistId: `${tidalArtistId}`
  });
  return await requestJson(`${con.apiUrl}/tidal/artists?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getQobuzArtist(qobuzArtistId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    artistId: `${qobuzArtistId}`
  });
  return await requestJson(`${con.apiUrl}/qobuz/artists?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
function sortAlbumsByDateDesc(albums) {
  return albums.toSorted((a, b) => {
    if (!a.dateReleased) return 1;
    if (!b.dateReleased) return -1;
    return b.dateReleased.localeCompare(a.dateReleased);
  });
}
async function getAllTidalArtistAlbums(tidalArtistId, setter, types, signal) {
  const albums = {
    lps: [],
    epsAndSingles: [],
    compilations: []
  };
  const promises = [];
  if (!types || types.find((t) => t === "LP")) {
    promises.push(
      (async () => {
        const page = await api.getTidalArtistAlbums(
          tidalArtistId,
          "LP",
          signal ?? null
        );
        albums.lps = page.items;
        if (setter) {
          const { lps, epsAndSingles, compilations } = albums;
          setter(
            sortAlbumsByDateDesc([
              ...lps,
              ...epsAndSingles,
              ...compilations
            ])
          );
        }
      })()
    );
  }
  if (!types || types.find((t) => t === "EPS_AND_SINGLES")) {
    promises.push(
      (async () => {
        const page = await api.getTidalArtistAlbums(
          tidalArtistId,
          "EPS_AND_SINGLES",
          signal ?? null
        );
        if (setter) {
          albums.epsAndSingles = page.items;
          const { lps, epsAndSingles, compilations } = albums;
          setter(
            sortAlbumsByDateDesc([
              ...lps,
              ...epsAndSingles,
              ...compilations
            ])
          );
        }
      })()
    );
  }
  if (!types || types.find((t) => t === "COMPILATIONS")) {
    promises.push(
      (async () => {
        const page = await api.getTidalArtistAlbums(
          tidalArtistId,
          "COMPILATIONS",
          signal ?? null
        );
        if (setter) {
          albums.compilations = page.items;
          const { lps, epsAndSingles, compilations } = albums;
          setter(
            sortAlbumsByDateDesc([
              ...lps,
              ...epsAndSingles,
              ...compilations
            ])
          );
        }
      })()
    );
  }
  await Promise.all(promises);
  return albums;
}
async function getAllQobuzArtistAlbums(qobuzArtistId, setter, types, signal) {
  const albums = {
    lps: [],
    epsAndSingles: [],
    compilations: []
  };
  const promises = [];
  if (!types || types.find((t) => t === "LP")) {
    promises.push(
      (async () => {
        const page = await api.getQobuzArtistAlbums(
          qobuzArtistId,
          "LP",
          signal ?? null
        );
        albums.lps = page.items;
        if (setter) {
          const { lps, epsAndSingles, compilations } = albums;
          setter(
            sortAlbumsByDateDesc([
              ...lps,
              ...epsAndSingles,
              ...compilations
            ])
          );
        }
      })()
    );
  }
  if (!types || types.find((t) => t === "EPS_AND_SINGLES")) {
    promises.push(
      (async () => {
        const page = await api.getQobuzArtistAlbums(
          qobuzArtistId,
          "EPS_AND_SINGLES",
          signal ?? null
        );
        if (setter) {
          albums.epsAndSingles = page.items;
          const { lps, epsAndSingles, compilations } = albums;
          setter(
            sortAlbumsByDateDesc([
              ...lps,
              ...epsAndSingles,
              ...compilations
            ])
          );
        }
      })()
    );
  }
  if (!types || types.find((t) => t === "COMPILATIONS")) {
    promises.push(
      (async () => {
        const page = await api.getQobuzArtistAlbums(
          qobuzArtistId,
          "COMPILATIONS",
          signal ?? null
        );
        if (setter) {
          albums.compilations = page.items;
          const { lps, epsAndSingles, compilations } = albums;
          setter(
            sortAlbumsByDateDesc([
              ...lps,
              ...epsAndSingles,
              ...compilations
            ])
          );
        }
      })()
    );
  }
  await Promise.all(promises);
  return albums;
}
async function getTidalArtistAlbums(tidalArtistId, albumType, signal) {
  const con = getConnection();
  const query = new QueryParams({
    artistId: `${tidalArtistId}`
  });
  if (albumType) {
    query.set("albumType", albumType);
  }
  return await requestJson(`${con.apiUrl}/tidal/artists/albums?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getQobuzArtistAlbums(qobuzArtistId, albumType, signal) {
  const con = getConnection();
  const query = new QueryParams({
    artistId: `${qobuzArtistId}`
  });
  if (albumType) {
    query.set("releaseType", albumType);
  }
  return await requestJson(`${con.apiUrl}/qobuz/artists/albums?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getAlbumFromTidalAlbumId(tidalAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    tidalAlbumId: `${tidalAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getAlbumFromQobuzAlbumId(qobuzAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    qobuzAlbumId: `${qobuzAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getLibraryAlbumsFromTidalArtistId(tidalArtistId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    tidalArtistId: `${tidalArtistId}`
  });
  return await requestJson(`${con.apiUrl}/menu/albums?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getLibraryAlbumsFromQobuzArtistId(qobuzArtistId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    qobuzArtistId: `${qobuzArtistId}`
  });
  return await requestJson(`${con.apiUrl}/menu/albums?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getTidalAlbum(tidalAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: `${tidalAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/tidal/albums?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getQobuzAlbum(qobuzAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: `${qobuzAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/qobuz/albums?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getTidalAlbumTracks(tidalAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: `${tidalAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/tidal/albums/tracks?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getQobuzAlbumTracks(qobuzAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: `${qobuzAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/qobuz/albums/tracks?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getYtAlbumTracks(ytAlbumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: `${ytAlbumId}`
  });
  return await requestJson(`${con.apiUrl}/yt/albums/tracks?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getTidalTrack(tidalTrackId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    trackId: `${tidalTrackId}`
  });
  return await requestJson(`${con.apiUrl}/tidal/track?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getTrackUrlForSource(trackId2, source, audioQuality, signal) {
  const con = getConnection();
  const query = new QueryParams({
    audioQuality,
    trackId: `${trackId2}`,
    source: `${source}`
  });
  const urls = await requestJson(
    `${con.apiUrl}/files/tracks/url?${query}`,
    {
      credentials: "include",
      signal: signal ?? null
    }
  );
  return urls[0];
}
async function addAlbumToLibrary(albumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: albumId.tidalAlbumId?.toString() ?? albumId.qobuzAlbumId,
    source: albumId.tidalAlbumId ? "TIDAL" : albumId.qobuzAlbumId ? "QOBUZ" : void 0
  });
  return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
async function removeAlbumFromLibrary(albumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: albumId.tidalAlbumId?.toString() ?? albumId.qobuzAlbumId,
    source: albumId.tidalAlbumId ? "TIDAL" : albumId.qobuzAlbumId ? "QOBUZ" : void 0
  });
  return await requestJson(`${con.apiUrl}/menu/album?${query}`, {
    method: "DELETE",
    credentials: "include",
    signal: signal ?? null
  });
}
async function refavoriteAlbum(albumId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    albumId: albumId.tidalAlbumId?.toString() ?? albumId.qobuzAlbumId,
    source: albumId.tidalAlbumId ? "TIDAL" : albumId.qobuzAlbumId ? "QOBUZ" : void 0
  });
  return await requestJson(`${con.apiUrl}/menu/album/re-favorite?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
async function retryDownload(taskId, signal) {
  const con = getConnection();
  const query = new QueryParams({
    taskId: `${taskId}`
  });
  return await requestJson(
    `${con.apiUrl}/downloader/retry-download?${query}`,
    {
      method: "POST",
      credentials: "include",
      signal: signal ?? null
    }
  );
}
async function download(items, source, signal) {
  const con = getConnection();
  const query = new QueryParams({
    trackId: items.trackId ? `${items.trackId}` : void 0,
    trackIds: items.trackIds ? `${items.trackIds.join(",")}` : void 0,
    albumId: items.albumId ? `${items.albumId}` : void 0,
    albumIds: items.albumIds ? `${items.albumIds.join(",")}` : void 0,
    source: `${source}`
  });
  return await requestJson(`${con.apiUrl}/downloader/download?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
async function getDownloadTasks(signal) {
  const con = getConnection();
  return await requestJson(`${con.apiUrl}/downloader/download-tasks`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function getTrackVisualization(track, source, max, signal) {
  const con = getConnection();
  const query = new QueryParams({
    trackId: `${trackId(track)}`,
    max: `${Math.ceil(max)}`,
    source: `${source}`
  });
  return await requestJson(
    `${con.apiUrl}/files/track/visualization?${query}`,
    {
      credentials: "include",
      signal: signal ?? null
    }
  );
}
async function getAudioZones(signal) {
  const con = getConnection();
  const query = new QueryParams({ offset: `0`, limit: `100` });
  return await requestJson(`${con.apiUrl}/audio-zone?${query}`, {
    credentials: "include",
    signal: signal ?? null
  });
}
async function createAudioZone(name, signal) {
  const con = getConnection();
  const query = new QueryParams({ name });
  return await requestJson(`${con.apiUrl}/audio-zone?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
async function updateAudioZone(update, signal) {
  const con = getConnection();
  return await requestJson(`${con.apiUrl}/audio-zone`, {
    method: "PATCH",
    body: JSON.stringify(update),
    credentials: "include",
    signal: signal ?? null
  });
}
async function deleteAudioZone(id, signal) {
  const con = getConnection();
  const query = new QueryParams({ id: `${id}` });
  return await requestJson(`${con.apiUrl}/audio-zone?${query}`, {
    method: "DELETE",
    credentials: "include",
    signal: signal ?? null
  });
}
async function runScan(origins, signal) {
  const con = getConnection();
  const query = new QueryParams({ origins: `${origins.join(",")}` });
  return await requestJson(`${con.apiUrl}/scan/run-scan?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
async function startScan(origins, signal) {
  const con = getConnection();
  const query = new QueryParams({ origins: `${origins.join(",")}` });
  return await requestJson(`${con.apiUrl}/scan/start-scan?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
async function enableScanOrigin(origin, signal) {
  const con = getConnection();
  const query = new QueryParams({ origin: `${origin}` });
  return await requestJson(`${con.apiUrl}/scan/scan-origins?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
async function addScanPath(path, signal) {
  const con = getConnection();
  const query = new QueryParams({ path: `${path}` });
  return await requestJson(`${con.apiUrl}/scan/scan-paths?${query}`, {
    method: "POST",
    credentials: "include",
    signal: signal ?? null
  });
}
class RequestError extends Error {
  constructor(response) {
    let message = `Request failed: ${response.status}`;
    if (response.statusText) {
      message += ` (${response.statusText})`;
    }
    if (response.url) {
      message += ` (url='${response.url}')`;
    }
    if (typeof response.redirected !== "undefined") {
      message += ` (redirected=${response.redirected})`;
    }
    if (response.headers) {
      message += ` (headers=${objToStr(response.headers)})`;
    }
    if (response.type) {
      message += ` (type=${response.type})`;
    }
    super(message);
    this.response = response;
  }
}
async function requestJson(url, options) {
  const con = getConnection();
  if (url[url.length - 1] === "?") url = url.substring(0, url.length - 1);
  const params = new QueryParams();
  const clientId = con.clientId;
  if (clientId) {
    params.set("clientId", clientId);
  }
  if (params.size > 0) {
    if (url.indexOf("?") > 0) {
      url += `&${params}`;
    } else {
      url += `?${params}`;
    }
  }
  const token = con.staticToken || con.token;
  const headers = {
    "Content-Type": "application/json",
    ...options?.headers ?? {}
  };
  if (token && !headers.Authorization) {
    headers.Authorization = token;
  }
  options = {
    ...options,
    headers
  };
  const response = await fetch(url, options);
  if (!response.ok) {
    throw new RequestError(response);
  }
  return await response.json();
}
const api = {
  getArtist,
  getArtistCover,
  getArtistSourceCover,
  getAlbum,
  getAlbums,
  getAllAlbums,
  getAlbumArtwork,
  getAlbumSourceArtwork,
  getAlbumTracks,
  getAlbumVersions,
  getTracks,
  getArtists,
  fetchSignatureToken,
  refetchSignatureToken,
  validateSignatureTokenAndClient,
  validateSignatureToken,
  magicToken,
  globalSearch,
  searchExternalMusicApi,
  searchAll,
  getArtistFromTidalArtistId,
  getArtistFromQobuzArtistId,
  getArtistFromTidalAlbumId,
  getAlbumFromTidalAlbumId,
  getAlbumFromQobuzAlbumId,
  getTidalArtist,
  getQobuzArtist,
  getAllTidalArtistAlbums,
  getAllQobuzArtistAlbums,
  getTidalArtistAlbums,
  getQobuzArtistAlbums,
  getLibraryAlbumsFromTidalArtistId,
  getLibraryAlbumsFromQobuzArtistId,
  getTidalAlbum,
  getQobuzAlbum,
  getTidalAlbumTracks,
  getQobuzAlbumTracks,
  getYtAlbumTracks,
  getTidalTrack,
  getTrackUrlForSource,
  addAlbumToLibrary,
  removeAlbumFromLibrary,
  refavoriteAlbum,
  getDownloadTasks,
  getTrackVisualization,
  retryDownload,
  download,
  getAudioZones,
  createAudioZone,
  updateAudioZone,
  deleteAudioZone,
  runScan,
  startScan,
  enableScanOrigin,
  addScanPath
};

clientAtom(
  true,
  "navigationBarExpanded"
);
const showAudioZones = clientAtom(false);
const showPlaybackSessions = clientAtom(false);
const showPlaybackQuality = clientAtom(false);
const showChangePlaybackTargetModal = clientAtom(false);
const showScanStatusBanner = clientAtom(false);
if (isServer) global.startupCallbacks = global.startupCallbacks ?? [];
else window.startupCallbacks = window.startupCallbacks ?? [];
function getStartupCallbacks() {
  if (isServer) {
    const x = globalThis.startupCallbacks;
    if (!x) globalThis.startupCallbacks = [];
    return globalThis.startupCallbacks;
  } else {
    const x = window.startupCallbacks;
    if (!x) window.startupCallbacks = [];
    return window.startupCallbacks;
  }
}
if (isServer) global.startedUp = global.startedUp ?? false;
else window.startedUp = window.startedUp ?? false;
function isStartedUp() {
  return (isServer ? globalThis.startedUp : window.startedUp) === true;
}
async function onStartup(func) {
  if (isStartedUp()) {
    try {
      await func();
    } catch (e) {
      console.error("Startup error:", e);
    }
    return;
  }
  getStartupCallbacks().push(func);
}
const [appState, setAppState] = createStore({
  connections: [],
  connection: void 0
});
createSignal();
createSignal();
connection.listen((con, prev) => {
  if (!con) return;
  if (con.token !== prev?.token || con.clientId !== prev?.clientId) {
    api.refetchSignatureToken();
  }
});
onStartup(async () => {
  const con = connection.get();
  if (con && con.token && con.clientId) {
    try {
      await api.validateSignatureToken();
    } catch (e) {
      console.debug("Failed to validateSignatureToken:", e);
    }
  }
});
onStartup(async () => {
  const zones = await api.getAudioZones();
  setPlayerState(
    produce((state) => {
      state.audioZones = zones.items;
      const current = currentPlaybackTarget();
      if (current?.type === "AUDIO_ZONE") {
        const existing = state.audioZones.find(
          (x) => x.id === current.audioZoneId
        );
        if (existing) {
          state.currentAudioZone = existing;
        }
      }
      if (!state.currentAudioZone && !currentPlaybackTarget()) {
        state.currentAudioZone = state.audioZones[0];
        if (state.currentAudioZone) {
          setCurrentPlaybackTarget({
            type: "AUDIO_ZONE",
            audioZoneId: state.currentAudioZone.id
          });
        }
      }
    })
  );
});

const onDownloadEventListener = createListener();
const onDownloadEvent = onDownloadEventListener.on;
const [downloadsState, setDownloadsState] = createStore({
  tasks: [],
  currentTasks: [],
  historyTasks: []
});
function handleDownloadEvent(event) {
  const eventType = event.type;
  switch (eventType) {
    case "SIZE":
      setDownloadsState(
        produce((state) => {
          const task = state.tasks.find(
            (task2) => task2.id === event.taskId
          );
          if (task) {
            task.totalBytes = event.bytes ?? task.totalBytes;
          }
        })
      );
      break;
    case "BYTES_READ":
      setDownloadsState(
        produce((state) => {
          const task = state.tasks.find(
            (task2) => task2.id === event.taskId
          );
          if (task) {
            task.bytes = event.total;
            task.progress = event.total / task.totalBytes * 100;
          }
        })
      );
      break;
    case "SPEED":
      setDownloadsState(
        produce((state) => {
          const task = state.tasks.find(
            (task2) => task2.id === event.taskId
          );
          if (task) {
            task.speed = event.bytesPerSecond;
          }
        })
      );
      break;
    case "STATE":
      setDownloadsState(
        produce((state) => {
          const task = state.tasks.find(
            (task2) => task2.id === event.taskId
          );
          if (task) {
            const prevState = task.state;
            task.state = event.state;
            if (isCurrent(task.state) && !isCurrent(prevState)) {
              const index = state.historyTasks.indexOf(task);
              if (index !== -1) {
                state.historyTasks.splice(index, 1);
              }
              state.currentTasks.unshift(task);
            } else if (!isCurrent(task.state) && isCurrent(prevState)) {
              const index = state.currentTasks.indexOf(task);
              if (index !== -1) {
                state.currentTasks.splice(index, 1);
              }
              state.historyTasks.unshift(task);
            }
            if (task.state === "FINISHED") {
              task.progress = 100;
            }
          }
        })
      );
      break;
    default:
      throw new Error(`Invalid DownloadEvent type: '${eventType}'`);
  }
}
onDownloadEvent(handleDownloadEvent);
function isCurrent(state) {
  return state === "STARTED" || state === "PAUSED" || state === "PENDING";
}
function isHistorical(state) {
  return !isCurrent(state);
}
onStartup(async () => {
  const tasks = await api.getDownloadTasks();
  const current = tasks.items.filter(({ state }) => isCurrent(state));
  const history = tasks.items.filter(({ state }) => isHistorical(state));
  setDownloadsState(
    produce((state) => {
      state.tasks = tasks.items;
      state.currentTasks = current;
      state.historyTasks = history;
    })
  );
});

const onScanEventListener = createListener();
const onScanEvent = onScanEventListener.on;
const [scanState, setScansState] = createStore({
  tasks: []
});
function handleScanEvent(event) {
  const eventType = event.type;
  switch (eventType) {
    case "FINISHED":
      setScansState(
        produce((state) => {
          const task = state.tasks.find(
            (task2) => deepEqual(task2.task, event.task)
          );
          if (task) {
            task.scanned = event.scanned;
            task.total = event.total;
          }
        })
      );
      setTimeout(() => {
        setScansState(
          produce((state) => {
            state.tasks = state.tasks.filter(
              (task) => !deepEqual(task.task, event.task)
            );
          })
        );
        if (scanState.tasks.length === 0) {
          showScanStatusBanner.set(false);
        }
      }, 5e3);
      break;
    case "COUNT":
      setScansState(
        produce((state) => {
          const task = state.tasks.find(
            (task2) => deepEqual(task2.task, event.task)
          );
          if (task) {
            task.scanned = event.scanned;
            task.total = event.total;
          } else {
            state.tasks.push({
              task: event.task,
              scanned: event.scanned,
              total: event.total
            });
          }
        })
      );
      showScanStatusBanner.set(true);
      break;
    case "SCANNED":
      setScansState(
        produce((state) => {
          const task = state.tasks.find(
            (task2) => deepEqual(task2.task, event.task)
          );
          if (task) {
            task.scanned = event.scanned;
            task.total = event.total;
          } else {
            state.tasks.push({
              task: event.task,
              scanned: event.scanned,
              total: event.total
            });
          }
        })
      );
      showScanStatusBanner.set(true);
      break;
    default:
      throw new Error(`Invalid ScanEvent type: '${eventType}'`);
  }
}
onScanEvent(handleScanEvent);
onStartup(async () => {
  setScansState(
    produce((state) => {
      state.tasks = [];
    })
  );
});

connection.listen((con) => {
  if (!con) return;
  updateWsUrl(
    con.apiUrl,
    con.clientId,
    Api.signatureToken(),
    con.staticToken
  );
  if (con.token && !Api.signatureToken()) {
    console.debug("Waiting for signature token");
    return;
  }
  wsService.reconnect();
});
Api.onSignatureTokenUpdated((signatureToken) => {
  const con = connection.get();
  if (!con) return;
  updateWsUrl(con.apiUrl, con.clientId, signatureToken, con.staticToken);
  if (con.token && !signatureToken) {
    console.debug("Waiting for signature token");
    return;
  }
  wsService.reconnect();
});
function updateWsUrl(apiUrl, clientId, signatureToken, staticToken) {
  if (!apiUrl?.startsWith("http")) return;
  const params = [];
  if (clientId) {
    params.push(`clientId=${encodeURIComponent(clientId)}`);
  }
  if (signatureToken) {
    params.push(`signature=${encodeURIComponent(signatureToken)}`);
  }
  if (staticToken) {
    params.push(`authorization=${encodeURIComponent(staticToken)}`);
  }
  wsUrl = `ws${apiUrl.slice(4)}/ws${params.length > 0 ? `?${params.join("&")}` : ""}`;
}
let ws;
let wsUrl;
const connectionId = clientAtom("", "ws.v1.connectionId");
const $connectionId = () => connectionId.get();
const connectionName = clientAtom(
  "New Connection",
  "ws.v1.connectionName"
);
const onConnectListener = createListener();
const onConnect = onConnectListener.on;
onConnect((id) => {
  if (!$connectionId()) {
    connectionId.set(id);
  }
  wsService.getSessions();
});
const onMessageListener = createListener();
const onMessageFirst = onMessageListener.onFirst;
onMessageFirst((data) => {
  console.debug("Received ws message", data);
  switch (data.type) {
    case "CONNECTION_ID" /* CONNECTION_ID */: {
      const message = data;
      onConnectListener.trigger(message.connectionId);
      break;
    }
    case "SESSIONS" /* SESSIONS */: {
      const message = data;
      setPlayerState(
        produce((state) => {
          state.playbackSessions = message.payload;
          const existing = message.payload.find(
            (p) => p.sessionId === state.currentPlaybackSession?.sessionId
          );
          if (existing) {
            updateSession(state, existing);
          } else if (typeof currentPlaybackSessionId() === "number") {
            const session = message.payload.find(
              (s) => s.sessionId === currentPlaybackSessionId()
            ) ?? message.payload[0];
            if (session) {
              updateSession(state, session, true);
            }
          } else {
            updateSession(state, message.payload[0], true);
          }
        })
      );
      break;
    }
    case "CONNECTIONS" /* CONNECTIONS */: {
      const message = data;
      setAppState(
        produce((state) => {
          state.connections = message.payload;
          state.connection = state.connections.find(
            (c) => c.connectionId === connectionId.get()
          );
        })
      );
      break;
    }
    case "SET_SEEK" /* SET_SEEK */: {
      const message = data;
      if (message.payload.sessionId === playerState.currentPlaybackSession?.sessionId) {
        seek(message.payload.seek);
      }
      break;
    }
    case "DOWNLOAD_EVENT" /* DOWNLOAD_EVENT */: {
      const message = data;
      onDownloadEventListener.trigger(message.payload);
      break;
    }
    case "SCAN_EVENT" /* SCAN_EVENT */: {
      const message = data;
      onScanEventListener.trigger(message.payload);
      break;
    }
    case "SESSION_UPDATED" /* SESSION_UPDATED */: {
      const message = data;
      const session = message.payload;
      setPlayerState(
        produce((state) => {
          updateSessionPartial(state, session);
        })
      );
      sessionUpdated(session);
      break;
    }
  }
});
const MAX_CONNECTION_RETRY_COUNT = -1;
const CONNECTION_RETRY_DEBOUNCE = 5e3;
const wsContext = {
  lastConnectionAttemptTime: 0,
  messageBuffer: []
};
const wsService = {
  ping() {
    this.send({ type: "PING" /* PING */ });
  },
  getConnectionId() {
    this.send({
      type: "GET_CONNECTION_ID" /* GET_CONNECTION_ID */
    });
  },
  registerConnection(connection2) {
    this.send({
      type: "REGISTER_CONNECTION" /* REGISTER_CONNECTION */,
      payload: connection2
    });
  },
  registerPlayers(players) {
    this.send({
      type: "REGISTER_PLAYERS" /* REGISTER_PLAYERS */,
      payload: players
    });
  },
  playbackAction(action) {
    this.send({
      type: "PLAYBACK_ACTION" /* PLAYBACK_ACTION */,
      payload: {
        action
      }
    });
  },
  createAudioZone(audioZone) {
    this.send({
      type: "CREATE_AUDIO_ZONE" /* CREATE_AUDIO_ZONE */,
      payload: {
        ...audioZone
      }
    });
  },
  getSessions() {
    this.send({
      type: "GET_SESSIONS" /* GET_SESSIONS */
    });
  },
  activateSession(sessionId) {
    this.updateSession({ sessionId, active: true });
  },
  createSession(session) {
    this.send({
      type: "CREATE_SESSION" /* CREATE_SESSION */,
      payload: {
        ...session,
        playlist: {
          ...session.playlist,
          tracks: session.playlist.tracks.map(toSessionPlaylistTrack)
        }
      }
    });
  },
  updateSession(session) {
    const payload = {
      ...session,
      playlist: void 0
    };
    if (session.playlist) {
      payload.playlist = {
        ...session.playlist
      };
    } else {
      delete payload.playlist;
    }
    this.send({
      type: "UPDATE_SESSION" /* UPDATE_SESSION */,
      payload
    });
  },
  deleteSession(sessionId) {
    this.send({
      type: "DELETE_SESSION" /* DELETE_SESSION */,
      payload: {
        sessionId
      }
    });
  },
  send(value) {
    if (ws) {
      console.debug("Sending WebSocket message", value);
      ws.send(JSON.stringify(value));
    } else {
      console.debug("Adding WebSocket message to buffer", value);
      wsContext.messageBuffer.push(value);
    }
  },
  newClient() {
    return new Promise((resolve, reject) => {
      console.log("connecting to ", wsUrl);
      const client = new WebSocket(wsUrl);
      let pingInterval;
      let opened = false;
      client.addEventListener("error", (e) => {
        console.error("WebSocket client error", e);
        if (!opened) {
          client.close();
          reject();
        }
      });
      client.addEventListener("open", (_e) => {
        const wasOpened = opened;
        opened = true;
        if (!wasOpened) {
          pingInterval = setInterval(
            () => {
              if (!opened) return clearInterval(pingInterval);
              this.ping();
            },
            9 * 60 * 1e3
          );
          ws = client;
          while (wsContext.messageBuffer.length > 0) {
            const value = wsContext.messageBuffer.shift();
            console.debug(
              "Sending buffered WebSocket message",
              value
            );
            ws.send(JSON.stringify(value));
          }
          this.getConnectionId();
          resolve();
        }
      });
      client.addEventListener(
        "message",
        (event) => {
          const data = JSON.parse(event.data);
          onMessageListener.trigger(data);
        }
      );
      client.addEventListener("close", async () => {
        if (opened) {
          console.debug("Closed WebSocket connection");
          opened = false;
          client.close();
          clearInterval(pingInterval);
          const now = Date.now();
          if (wsContext.lastConnectionAttemptTime + 5e3 > now) {
            console.debug(
              `Debouncing connection retry attempt. Waiting ${CONNECTION_RETRY_DEBOUNCE}ms`
            );
            await this.sleep(CONNECTION_RETRY_DEBOUNCE);
          }
          wsContext.lastConnectionAttemptTime = now;
          await this.attemptConnection();
        } else {
          reject();
        }
      });
    });
  },
  async sleep(ms) {
    return new Promise((resolve) => {
      setTimeout(resolve, ms);
    });
  },
  async attemptConnection() {
    let attemptNumber = 0;
    while (true) {
      console.debug(
        `Attempting connection${attemptNumber > 0 ? `, Attempt ${attemptNumber + 1}` : ""}`
      );
      try {
        await this.newClient();
        console.debug("Successfully connected client");
        return;
      } catch (e) {
        if (attemptNumber++ === MAX_CONNECTION_RETRY_COUNT && MAX_CONNECTION_RETRY_COUNT !== -1) {
          break;
        }
        console.error(
          `WebSocket connection failed at '${wsUrl}':`,
          objToStr(e)
        );
        console.debug(
          `Failed to connect. Waiting ${CONNECTION_RETRY_DEBOUNCE}ms`
        );
        await this.sleep(CONNECTION_RETRY_DEBOUNCE);
      }
    }
    throw new Error("Failed to establish connection to websocket server");
  },
  reconnect() {
    if (ws) ws.close();
    return this.attemptConnection();
  }
};

async function responsePromise() {
  return new Promise((resolve) => {
  });
}

let tryingToPlay = false;
let audio;
function initSilence() {
  console.debug("initSilence");
  const query = new QueryParams({
    duration: `${5}`,
    format: Api.AudioFormat.MP3
  });
  const con = getConnection();
  const clientIdParam = con.clientId;
  const signatureToken = Api.signatureToken();
  if (con.clientId && signatureToken) {
    query.set("clientId", clientIdParam);
    query.set("signature", signatureToken);
  }
  if (con.staticToken) {
    query.set("authorization", con.staticToken);
  }
  const url = `${con.apiUrl}/files/silence?${query}`;
  audio = new Audio(url);
  audio.loop = true;
  audio.play();
  audio.addEventListener("error", (e) => {
    console.error("Failed to start audio:", e.error);
    tryingToPlay = false;
    audio = void 0;
  });
}
function isSilencePlaying() {
  return tryingToPlay || audio?.paused === false;
}
function startSilence() {
  console.debug("startSilence");
  if (isSilencePlaying()) {
    console.debug("startSilence: already playing");
    return;
  }
  tryingToPlay = true;
  initSilence();
}
function stopSilence() {
  console.debug("stopSilence");
  tryingToPlay = false;
  if (!isSilencePlaying()) {
    console.debug("stopSilence: already not playing");
    return;
  }
}

const [playerState, setPlayerState] = createStore({
  playing: false,
  currentPlaybackSession: void 0,
  playbackSessions: [],
  currentAudioZone: void 0,
  audioZones: [],
  currentTrack: void 0
});
const [_playbackQuality, _setPlaybackQuality] = makePersisted(
  createSignal(
    { format: Api.AudioFormat.SOURCE },
    { equals: false }
  ),
  {
    name: `player.v1.playbackQuality`
  }
);
const playbackQuality = _playbackQuality;
const [_currentPlaybackTarget, _setCurrentPlaybackTarget] = makePersisted(
  createSignal(void 0, {
    equals: false
  }),
  {
    name: `player.v1.currentPlaybackTarget`
  }
);
const onCurrentPlaybackTargetChangedListener = createListener();
const currentPlaybackTarget = _currentPlaybackTarget;
const setCurrentPlaybackTarget = (value, trigger = true) => {
  const old = _currentPlaybackTarget();
  if (typeof value === "function") {
    value = value(old);
  }
  _setCurrentPlaybackTarget(value);
  if (trigger && value !== old) {
    onCurrentPlaybackTargetChangedListener.trigger(value, old);
  }
  updatePlayback({});
};
const [currentPlaybackSessionId, setCurrentPlaybackSessionId] = makePersisted(
  createSignal(void 0, { equals: false }),
  {
    name: `player.v1.currentPlaybackSessionId`
  }
);
createSignal();
function setVolume(volume) {
  console.debug("Setting volume to", volume);
  updatePlayback({ volume });
}
const [_currentSeek, _setCurrentSeek] = makePersisted(
  createSignal(void 0, { equals: false }),
  {
    name: `player.v1.currentSeek`
  }
);
const onCurrentSeekChangedListener = createListener();
const onCurrentSeekChanged = onCurrentSeekChangedListener.on;
const currentSeek = _currentSeek;
const [_currentTrackLength, _setCurrentTrackLength] = makePersisted(
  createSignal(0, { equals: false }),
  {
    name: `player.v1.currentTrackLength`
  }
);
const onCurrentTrackLengthChangedListener = createListener();
const currentTrackLength = _currentTrackLength;
const setCurrentTrackLength = (value, trigger = true) => {
  const old = _currentTrackLength();
  if (typeof value === "function") {
    value = value(old);
  }
  _setCurrentTrackLength(value);
  if (trigger && value !== old) {
    onCurrentTrackLengthChangedListener.trigger(value, old);
  }
};
makePersisted(
  createSignal(void 0, {
    equals: false
  }),
  {
    name: `player.v2.currentAlbum`
  }
);
const [_playlistPosition, _setPlaylistPosition] = makePersisted(
  createSignal(void 0, { equals: false }),
  { name: `player.v1.playlistPosition` }
);
const playlistPosition = _playlistPosition;
const [_playlist, _setPlaylist] = makePersisted(
  createSignal([], { equals: false }),
  { name: `player.v1.playlist` }
);
const playlist$1 = _playlist;
function isMasterPlayer(zone) {
  const activeZonePlayers = getActiveZonePlayers(zone);
  console.debug(
    "isMasterPlayer:",
    "zone:",
    zone,
    "players:",
    players,
    "activeZonePlayers:",
    activeZonePlayers
  );
  if (activeZonePlayers.length === 0) {
    console.debug("isMasterPlayer: no active zone players");
    return false;
  }
  const first = activeZonePlayers[0];
  if (zone?.players.findIndex((p) => p.playerId === first?.id) !== 0) {
    console.debug("isMasterPlayer: player is not first");
    return false;
  }
  console.debug("isMasterPlayer: player is master");
  return true;
}
function getActiveZonePlayers(zone) {
  console.debug("getActiveZonePlayers: zone =", zone, "players =", players);
  return players.filter((p) => zone?.players.some((x) => p.id === x.playerId)) ?? [];
}
function isActiveConnectionPlayer(playbackTarget) {
  const players2 = getActiveConnectionPlayers(
    appState.connection,
    playbackTarget
  );
  return players2.length === 1;
}
function getActiveConnectionPlayers(connection, playbackTarget) {
  console.debug(
    "getActiveConnectionPlayers: connection =",
    connection,
    "players =",
    players
  );
  if (connection?.connectionId !== playbackTarget.connectionId) {
    return [];
  }
  return players.filter(
    (p) => connection?.players.some(
      (x) => p.id === x.playerId && x.audioOutputId === playbackTarget.outputId
    )
  ) ?? [];
}
async function play() {
  console.debug("Play called");
  await updatePlayback({ playing: true });
}
const seekListener = createListener();
const onSeek = seekListener.on;
const offSeek = seekListener.off;
async function seek(seek2, manual = false) {
  console.debug("Seek called");
  if (typeof seek2 === "number" && manual) {
    console.debug(`Setting seek to ${seek2}`);
    await updatePlayback({ play: playing(), seek: seek2 });
  }
  seekListener.trigger(seek2, manual);
}
async function pause() {
  console.debug("Pause called");
  await updatePlayback({ playing: false });
}
const prevTrackListener = createListener();
const onPreviousTrack = prevTrackListener.on;
const offPreviousTrack = prevTrackListener.off;
async function previousTrack() {
  if (playlistPosition() === 0) {
    console.debug("Setting track position to 0");
    seek(0, true);
  } else if ((currentSeek() ?? 0) < 5) {
    console.debug("Playing previous track");
    const position = playlistPosition() ?? 0;
    await updatePlayback({
      play: true,
      seek: 0,
      position: position > 0 ? position - 1 : position
    });
  } else {
    console.debug("Setting track position to 0");
    seek(0, true);
  }
  return false;
}
const nextTrackListener = createListener();
const onNextTrack = nextTrackListener.on;
const offNextTrack = nextTrackListener.off;
async function nextTrack() {
  if (typeof playlistPosition() === "number" && playlistPosition() < playlist$1().length - 1) {
    console.debug("Playing next track");
    const position = playlistPosition() ?? 0;
    await updatePlayback({
      play: true,
      seek: 0,
      position: position + 1
    });
  } else {
    console.debug("No next track to play");
    stop();
  }
  return false;
}
async function stop() {
  await updatePlayback({ stop: false });
}
const players = [];
function sessionUpdated(update) {
  const playbackTarget = update.playbackTarget;
  const playbackTargetType = playbackTarget.type;
  switch (playbackTargetType) {
    case "AUDIO_ZONE":
      {
        if (!isMasterPlayer(
          playerState.audioZones.find(
            (z) => z.id === playbackTarget.audioZoneId
          )
        )) {
          handlePlaybackUpdate(update);
          console.debug("Not master player. Returning");
          return;
        }
      }
      break;
    case "CONNECTION_OUTPUT":
      if (!isActiveConnectionPlayer(playbackTarget)) {
        handlePlaybackUpdate(update);
        console.debug("Not active connection player. Returning");
        return;
      }
      break;
    default:
      throw new Error(
        `Invalid playbackTargetType: '${playbackTargetType}'`
      );
  }
  const sessionId = update.sessionId;
  const playbackUpdate = {
    sessionId,
    playbackTarget
  };
  for (const [key, value] of orderedEntries(update, [
    "play",
    "stop",
    "playing",
    "playlist",
    "position",
    "seek",
    "volume"
  ])) {
    if (typeof value === "undefined") continue;
    switch (key) {
      case "play":
        playbackUpdate.play = value;
        break;
      case "stop":
        playbackUpdate.stop = value;
        break;
      case "playing":
        playbackUpdate.playing = value;
        break;
      case "playlist":
        playbackUpdate.tracks = value?.tracks;
        break;
      case "position":
        playbackUpdate.position = value;
        break;
      case "seek":
        playbackUpdate.seek = value;
        break;
      case "volume":
        playbackUpdate.volume = value;
        break;
      case "quality":
        playbackUpdate.quality = value;
        break;
    }
  }
  updatePlayback(playbackUpdate, false);
}
async function confirmChangePlaybackTarget() {
  showChangePlaybackTargetModal.set(true);
  return new Promise(async (resolve) => {
    resolve(await responsePromise());
  });
}
async function updatePlayback(update, updateSession2 = true) {
  if (!update.quality) {
    update.quality = playbackQuality();
  }
  const playbackUpdate = update;
  const sessionId = playbackUpdate.sessionId ?? currentPlaybackSessionId();
  const session = playerState.playbackSessions.find(
    (x) => x.sessionId === sessionId
  );
  let playbackTarget = playbackUpdate.playbackTarget;
  const currentTarget = currentPlaybackTarget();
  let useDefaultPlaybackTarget = false;
  if (session) {
    playbackTarget = session.playbackTarget;
    if (currentTarget && !deepEqual(currentTarget, session.playbackTarget) && !session.playing && (update.playing || update.play) && await confirmChangePlaybackTarget()) {
      useDefaultPlaybackTarget = true;
    }
  }
  if (useDefaultPlaybackTarget) {
    if (currentTarget) {
      playbackTarget = currentTarget;
    }
  }
  if (updateSession2) {
    const sessionUpdate = {
      sessionId,
      playbackTarget
    };
    for (const [key, value] of orderedEntries(update, [
      "play",
      "playing",
      "position",
      "seek",
      "volume",
      "tracks",
      "quality"
    ])) {
      if (typeof value === "undefined") continue;
      switch (key) {
        case "play":
          sessionUpdate.play = value;
          if (update.play) {
            sessionUpdate.playing = true;
          }
          break;
        case "stop":
          sessionUpdate.stop = value;
          break;
        case "playing":
          sessionUpdate.playing = value;
          break;
        case "position":
          sessionUpdate.position = value;
          break;
        case "seek":
          sessionUpdate.seek = value;
          break;
        case "volume":
          sessionUpdate.volume = value;
          break;
        case "tracks":
          sessionUpdate.playlist = {
            tracks: value
          };
          break;
        case "quality":
          sessionUpdate.quality = value;
          break;
      }
    }
    updatePlaybackSession(sessionId, sessionUpdate);
  }
  const activePlayers = [];
  const playbackTargetType = playbackTarget.type;
  switch (playbackTargetType) {
    case "AUDIO_ZONE":
      activePlayers.push(
        ...getActiveZonePlayers(
          playerState.audioZones.find(
            ({ id }) => id === playbackTarget.audioZoneId
          )
        )
      );
      break;
    case "CONNECTION_OUTPUT":
      activePlayers.push(
        ...getActiveConnectionPlayers(
          appState.connection,
          playbackTarget
        )
      );
      break;
    default:
      throw new Error(
        `Invalid playbackTargetType: '${playbackTargetType}'`
      );
  }
  console.debug("activePlayers:", activePlayers);
  await updateActivePlayers(activePlayers, {
    ...update,
    sessionId,
    playbackTarget
  });
}
async function updateActivePlayers(activePlayers, update) {
  if (activePlayers.length === 0) {
    handlePlaybackUpdate(update);
  } else {
    stopSilence();
  }
  await Promise.all(
    activePlayers.map(
      (activePlayer) => activePlayer.updatePlayback(update)
    )
  );
}
function handlePlaybackUpdate(update) {
  for (const [key, value] of orderedEntries(update, [
    "stop",
    "volume",
    "seek",
    "play",
    "tracks",
    "position",
    "playing",
    "quality"
  ])) {
    if (typeof value === "undefined") continue;
    switch (key) {
      case "stop":
        if (update.play || update.playing) continue;
        if (navigator.mediaSession) {
          navigator.mediaSession.playbackState = "paused";
        }
        break;
      case "playing":
        if (update.play) continue;
        if (navigator.mediaSession) {
          navigator.mediaSession.playbackState = update.playing ? "playing" : "paused";
        }
        if (update.playing) {
          return startSilence();
        }
        break;
      case "play":
        if (!isSilencePlaying()) {
          return startSilence();
        }
        if (navigator.mediaSession) {
          navigator.mediaSession.playbackState = "playing";
        }
        break;
      case "seek":
        if (typeof update.seek === "number") {
          navigator.mediaSession?.setPositionState({
            position: update.seek,
            duration: currentTrackLength()
          });
        }
        break;
    }
  }
  const session = playerState.playbackSessions.find(
    (session2) => session2.sessionId === update.sessionId
  );
  if (session?.playing) {
    if (!isSilencePlaying()) {
      return startSilence();
    }
  }
}
function updatePlaybackSession(id, request) {
  console.debug("updatePlaybackSession:", id, request);
  setPlayerState(
    produce((state) => {
      const current = state.currentPlaybackSession;
      const session = current?.sessionId === id ? current : state.playbackSessions.find((s) => s.sessionId === id);
      if (session) {
        const { playlist: playlist2 } = session;
        if (playlist2 && request.playlist) {
          request.playlist.sessionPlaylistId = playlist2.sessionPlaylistId;
        }
        updateSessionPartial(state, request);
        const updatePlaybackSession2 = {
          ...request,
          playlist: void 0
        };
        if (request.playlist) {
          updatePlaybackSession2.playlist = {
            ...request.playlist,
            sessionPlaylistId: request.playlist.sessionPlaylistId,
            tracks: request.playlist.tracks.map(
              toSessionPlaylistTrack
            )
          };
          console.debug(
            "updatePlaybackSession: playlist:",
            updatePlaybackSession2.playlist
          );
        } else {
          delete updatePlaybackSession2.playlist;
        }
        wsService.updateSession(updatePlaybackSession2);
      }
    })
  );
}
const onCurrentPlaybackSessionChangedListener = createListener();
const onCurrentPlaybackSessionChanged = onCurrentPlaybackSessionChangedListener.on;
const onUpdateSessionPartialListener = createListener();
const onUpdateSessionPartial = onUpdateSessionPartialListener.on;
function updateSessionPartial(state, session) {
  state.playbackSessions.forEach((s) => {
    if (s.sessionId === session.sessionId) {
      Object.assign(s, session);
    }
  });
  if (state.currentPlaybackSession?.sessionId === session.sessionId) {
    Object.assign(state.currentPlaybackSession, session);
    let updatedPlaylist = false;
    if (typeof session.seek !== "undefined") {
      _setCurrentSeek(session.seek);
    }
    if (typeof session.position !== "undefined") {
      _setPlaylistPosition(session.position);
      updatedPlaylist = true;
    }
    if (typeof session.playlist !== "undefined") {
      _setPlaylist(session.playlist.tracks);
      updatedPlaylist = true;
    }
    if (updatedPlaylist) {
      if (typeof playlistPosition() === "number") {
        const track = state.currentPlaybackSession.playlist.tracks[playlistPosition()];
        if (track) {
          state.currentTrack = track;
          setCurrentTrackLength(Math.round(track.duration));
        }
      } else {
        state.currentTrack = void 0;
        setCurrentTrackLength(0);
      }
    }
  }
  onUpdateSessionPartialListener.trigger(session);
}
function updateSession(state, session, setAsCurrent = false) {
  state.playbackSessions.forEach((s) => {
    if (s.sessionId === session.sessionId) {
      Object.assign(s, session);
    }
  });
  if (setAsCurrent || session.sessionId === state.currentPlaybackSession?.sessionId) {
    const old = state.currentPlaybackSession;
    state.currentPlaybackSession = session;
    setCurrentPlaybackSessionId(session.sessionId);
    console.debug("session changed to", session, "from", old);
    _setPlaylist(session.playlist.tracks);
    _setCurrentSeek(session.seek);
    _setPlaylistPosition(
      session.playlist.tracks.length > 0 ? session.position : void 0
    );
    if (typeof playlistPosition() === "number") {
      const track = session.playlist.tracks[playlistPosition()];
      if (track) {
        state.currentTrack = track;
        setCurrentTrackLength(Math.round(track.duration));
      }
    } else {
      state.currentTrack = void 0;
      setCurrentTrackLength(0);
    }
    onCurrentPlaybackSessionChangedListener.trigger(session, old);
  }
}
onCurrentSeekChanged((value, old) => {
  console.debug("current seek changed from", old, "to", value);
  if (typeof value === "number") {
    navigator.mediaSession?.setPositionState({
      position: value,
      duration: currentTrackLength()
    });
  }
  const activeZonePlayer = playerState.audioZones.some(
    (zone) => isMasterPlayer(zone)
  );
  const playbackTarget = currentPlaybackTarget();
  if (activeZonePlayer || playbackTarget?.type === "CONNECTION_OUTPUT" && isActiveConnectionPlayer(playbackTarget)) {
    updatePlayback({ seek: value ?? 0 });
  }
});
onUpdateSessionPartial((session) => {
  if (playerState.currentPlaybackSession?.sessionId !== session.sessionId) {
    return;
  }
  if (typeof session.seek !== "undefined") {
    _setCurrentSeek(session.seek);
  }
});
function playing() {
  return playerState.currentPlaybackSession?.playing ?? false;
}
if (!isServer) {
  if (navigator?.mediaSession) {
    onCurrentPlaybackSessionChanged((value) => {
      navigator.mediaSession.playbackState = value?.playing ? "playing" : "paused";
      console.debug(
        "updated playback state to",
        navigator.mediaSession.playbackState
      );
    });
    navigator.mediaSession.setActionHandler("play", () => {
      console.log("mediaSession: play");
      play();
    });
    navigator.mediaSession.setActionHandler("pause", () => {
      console.log("mediaSession: pause");
      if (navigator.mediaSession.playbackState === "playing") {
        pause();
      } else {
        play();
      }
    });
    navigator.mediaSession.setActionHandler("stop", () => {
      console.log("mediaSession: stop");
      stop();
    });
    navigator.mediaSession.setActionHandler("nexttrack", () => {
      console.log("mediaSession: nexttrack");
      nextTrack();
    });
    navigator.mediaSession.setActionHandler("previoustrack", () => {
      console.log("mediaSession: previoustrack");
      previousTrack();
    });
  }
  document.body.onkeydown = function(e) {
    const target = e.target;
    if (!(target instanceof HTMLInputElement) && (e.key == " " || e.code == "Space")) {
      if (playerState.currentPlaybackSession?.playing || playing()) {
        pause();
      } else {
        play();
      }
      e.preventDefault();
    }
  };
}

function zeroPad(num, places) {
  return String(num).padStart(places, "0");
}
function toTime(value) {
  const seconds = Math.round(value);
  const minutes = ~~(seconds / 60);
  const minutesAndSeconds = `${minutes % 60}:${zeroPad(seconds % 60, 2)}`;
  if (minutes >= 60) {
    const pad = minutes % 60 < 10 ? "0" : "";
    return `${~~(minutes / 60)}:${pad}${minutesAndSeconds}`;
  }
  return minutesAndSeconds;
}
function displayAlbumVersionQuality(version) {
  let str = "";
  switch (version.source) {
    case Api.TrackSource.LOCAL:
      break;
    case Api.TrackSource.TIDAL:
      str += "Tidal";
      break;
    case Api.TrackSource.QOBUZ:
      str += "Qobuz";
      break;
    case Api.TrackSource.YT:
      str += "YouTube Music";
      break;
    default:
      version.source;
  }
  if (version.format) {
    if (str.length > 0) {
      str += " ";
    }
    switch (version.format) {
      case Api.AudioFormat.AAC:
        str += "AAC";
        break;
      case Api.AudioFormat.FLAC:
        str += "FLAC";
        break;
      case Api.AudioFormat.MP3:
        str += "MP3";
        break;
      case Api.AudioFormat.OPUS:
        str += "OPUS";
        break;
      case Api.AudioFormat.SOURCE:
        break;
      default:
        version.format;
    }
  }
  if (version.sampleRate) {
    if (str.length > 0) {
      str += " ";
    }
    str += `${version.sampleRate / 1e3} kHz`;
  }
  if (version.bitDepth) {
    if (str.length > 0) {
      str += ", ";
    }
    str += `${version.bitDepth}-bit`;
  }
  return str;
}
function displayAlbumVersionQualities(versions, maxCharacters = 25) {
  let str = displayAlbumVersionQuality(versions[0]);
  let count = 1;
  for (let i = 1; i < versions.length; i++) {
    const display = displayAlbumVersionQuality(versions[i]);
    if (str.length + display.length + " / ".length > maxCharacters) break;
    str += " / " + display;
    count++;
  }
  if (versions.length - count > 0) {
    str += ` (+${versions.length - count})`;
  }
  return str;
}

function artistRoute(artist2) {
  const artistType = artist2.type;
  switch (artistType) {
    case "LIBRARY":
      if ("artistId" in artist2) {
        return `/artists?artistId=${artist2.artistId}`;
      } else {
        return `/artists?artistId=${artist2.id}`;
      }
    case "TIDAL":
      if ("artistId" in artist2) {
        return `/artists?tidalArtistId=${artist2.artistId}`;
      } else {
        return `/artists?tidalArtistId=${artist2.id}`;
      }
    case "QOBUZ":
      if ("artistId" in artist2) {
        return `/artists?qobuzArtistId=${artist2.artistId}`;
      } else {
        return `/artists?qobuzArtistId=${artist2.id}`;
      }
    case "YT":
      if ("artistId" in artist2) {
        return `/artists?ytArtistId=${artist2.artistId}`;
      } else {
        return `/artists?ytArtistId=${artist2.id}`;
      }
    default:
      throw new Error(`Invalid artistType: ${artistType}`);
  }
}

var _tmpl$$4 = ["<div", ' class="album-controls"><button class="media-button play-button button"><img src="/img/play-button.svg" alt="Play"></button><button class="media-button options-button button"><img src="/img/more-options.svg" alt="Play"></button></div>'], _tmpl$2$2 = ["<div", ' class="album-details"><!--$-->', "<!--/--><!--$-->", "<!--/--><!--$-->", "<!--/--><!--$-->", "<!--/--></div>"], _tmpl$3$2 = ["<div", ' class="album-title">', "</div>"], _tmpl$4$1 = ["<a", ' class="album-title-text" title="', '"><!--$-->', "<!--/--><!--$-->", "<!--/--></a>"], _tmpl$5$1 = ["<span", ' class="album-details-explicit-wordwrap"><!--$-->', "<!--/--><!--$-->", "<!--/--></span>"], _tmpl$6$1 = ["<img", ' class="album-details-explicit" src="/img/explicit.svg" alt="Explicit">'], _tmpl$7$1 = ["<span", ' class="album-title-text" title="', '">', "</span>"], _tmpl$8 = ["<div", ' class="album-artist"><a', ' class="album-artist-text">', "</a></div>"], _tmpl$9 = ["<div", ' class="album-year"><span class="album-year-text">', "</span></div>"], _tmpl$10 = ["<div", ' class="album-version-qualities"><span class="album-version-qualities-text">', "</span></div>"], _tmpl$11 = ["<img", ' class="album-icon" style="', '"', ' alt="', '" title="', '" loading="lazy">'], _tmpl$12 = ["<div", ' class="album"><div class="album-icon-container" style="', '">', "</div><!--$-->", "<!--/--></div>"], _tmpl$13 = ["<a", "><!--$-->", "<!--/--><!--$-->", "<!--/--></a>"];
function albumControls(album2) {
  return ssr(_tmpl$$4, ssrHydrationKey());
}
function getAlbumTitleDisplay(props) {
  const albumType = props.album.type;
  switch (albumType) {
    case "LIBRARY":
      return props.album.title;
    case "TIDAL": {
      let title = props.album.title;
      if (props.album.mediaMetadataTags?.includes("DOLBY_ATMOS")) {
        title += " (Dolby Atmos)";
      }
      return title;
    }
    case "QOBUZ":
      return props.album.title;
    case "YT":
      return props.album.title;
    default:
      throw new Error(`Invalid albumType: ${albumType}`);
  }
}
function isExplicit(props) {
  const albumType = props.album.type;
  switch (albumType) {
    case "LIBRARY":
      return false;
    case "TIDAL":
      return props.album.explicit;
    case "QOBUZ":
      return props.album.parentalWarning;
    case "YT":
      return false;
    default:
      throw new Error(`Invalid albumType: ${albumType}`);
  }
}
const wordsCache = {};
function getWords(str) {
  const words = wordsCache[str] ?? str.split(" ");
  wordsCache[str] = words;
  return words;
}
function allButLastWord(str) {
  const words = getWords(str);
  return words.slice(0, words.length - 1).join(" ");
}
function lastWord(str) {
  const words = getWords(str);
  return words[words.length - 1];
}
function albumDetails(props) {
  return ssr(_tmpl$2$2, ssrHydrationKey(), props.title && ssr(_tmpl$3$2, ssrHydrationKey(), props.route ? ssr(_tmpl$4$1, ssrHydrationKey() + ssrAttribute("href", escape(albumRoute(props.album), true), false), `${escape(props.album.title, true)}${isExplicit(props) ? " (Explicit)" : ""}`, escape(allButLastWord(getAlbumTitleDisplay(props))), lastWord(getAlbumTitleDisplay(props)) ? escape([" ", ssr(_tmpl$5$1, ssrHydrationKey(), escape(lastWord(getAlbumTitleDisplay(props))), isExplicit(props) && _tmpl$6$1[0] + ssrHydrationKey() + _tmpl$6$1[1])]) : isExplicit(props) && _tmpl$6$1[0] + ssrHydrationKey() + _tmpl$6$1[1]) : ssr(_tmpl$7$1, ssrHydrationKey(), `${escape(props.album.title, true)}${isExplicit(props) ? " (Explicit)" : ""}`, escape(props.album.title))), props.artist && ssr(_tmpl$8, ssrHydrationKey(), ssrAttribute("href", escape(artistRoute(props.album), true), false), escape(props.album.artist)), props.year && "dateReleased" in props.album && ssr(_tmpl$9, ssrHydrationKey(), escape(props.album.dateReleased?.substring(0, 4))), "versions" in props.album && props.versionQualities && ssr(_tmpl$10, ssrHydrationKey(), props.album.versions.length > 0 && escape(displayAlbumVersionQualities(props.album.versions))));
}
function albumRoute(album2) {
  const albumType = album2.type;
  switch (albumType) {
    case "LIBRARY":
      if ("albumId" in album2) {
        return `/albums?albumId=${album2.albumId}`;
      } else if ("id" in album2) {
        return `/albums?albumId=${album2.id}`;
      } else {
        throw new Error(`Invalid album: ${album2}`);
      }
    case "TIDAL":
      if ("number" in album2) {
        return `/albums?tidalAlbumId=${album2.albumId}`;
      } else {
        return `/albums?tidalAlbumId=${album2.id}`;
      }
    case "QOBUZ":
      if ("number" in album2) {
        return `/albums?qobuzAlbumId=${album2.albumId}`;
      } else {
        return `/albums?qobuzAlbumId=${album2.id}`;
      }
    case "YT":
      if ("number" in album2) {
        return `/albums?ytAlbumId=${album2.albumId}`;
      } else {
        return `/albums?ytAlbumId=${album2.id}`;
      }
    default:
      throw new Error(`Invalid albumType: ${albumType}`);
  }
}
function albumImage(props, blur) {
  return ssr(_tmpl$11, ssrHydrationKey(), `width:${escape(props.size, true)}px;height:${escape(props.size, true)}px` + (";image-rendering:" + (blur ? "pixelated" : escape(void 0, true))) + (";cursor:" + (props.onClick ? "pointer" : escape(void 0, true))), ssrAttribute("src", escape(api.getAlbumArtwork(props.album, blur ? 16 : props.imageRequestSize, blur ? 16 : props.imageRequestSize), true), false), `${escape(props.album.title, true)} by ${escape(props.album.artist, true)}`, `${escape(props.album.title, true)} by ${escape(props.album.artist, true)}`);
}
function album(props) {
  props.size = props.size ?? 200;
  props.imageRequestSize = props.imageRequestSize ?? Math.ceil(Math.round(Math.max(200, props.size) * 1.33) / 20) * 20;
  props.artist = props.artist ?? false;
  props.title = props.title ?? false;
  props.route = props.route ?? true;
  props.year = props.year ?? false;
  props.versionQualities = props.versionQualities ?? false;
  const fullProps = props;
  const [blur, setBlur] = createSignal(false);
  createComputed(() => {
    setBlur(typeof fullProps.blur === "boolean" ? fullProps.blur : "blur" in fullProps.album && fullProps.album.blur);
  });
  return ssr(_tmpl$12, ssrHydrationKey(), `width:${escape(fullProps.size, true)}px;height:${escape(fullProps.size, true)}px`, fullProps.route ? ssr(_tmpl$13, ssrHydrationKey() + ssrAttribute("href", escape(albumRoute(fullProps.album), true), false), escape(albumImage(fullProps, blur())), fullProps.controls && escape(albumControls(fullProps.album))) : escape([albumImage(fullProps, blur()), fullProps.controls && albumControls(fullProps.album)]), (fullProps.artist || fullProps.title) && escape(albumDetails(fullProps)));
}

var _tmpl$$3 = ["<div", ' class="playlist"><div class="playlist-tracks"><div class="playlist-tracks-play-queue">Play queue</div><!--$-->', "<!--/--></div></div>"], _tmpl$2$1 = ["<div", ' class="', '"><div class="playlist-tracks-track-album-artwork"><div class="playlist-tracks-track-album-artwork-icon"><!--$-->', "<!--/--><!--$-->", '<!--/--></div></div><div class="playlist-tracks-track-details"><div class="playlist-tracks-track-details-title">', '</div><div class="playlist-tracks-track-details-artist">', "</div></div><!--$-->", "<!--/--></div>"], _tmpl$3$1 = ["<div", ' class="playlist-tracks-playing-from">Playing from: <a href="', '">', "</a></div>"], _tmpl$4 = ["<div", ' class="playlist-tracks-next-up">Next up:</div>'], _tmpl$5 = ["<img", ' class="audio-icon" src="/img/audio-white.svg" alt="Playing">'], _tmpl$6 = ["<img", ' class="play-icon" src="/img/play-button-white.svg" alt="Playing">'], _tmpl$7 = ["<div", ' class="playlist-tracks-track-remove"><img class="cross-icon" src="/img/cross-white.svg" alt="Remove from queue"></div>'];
function playlist() {
  const [playlist2, setPlaylist] = createSignal([]);
  const [currentlyPlayingIndex, setCurrentlyPlayingIndex] = createSignal();
  function updateCurrentlyPlayingIndex() {
    setCurrentlyPlayingIndex(playlist2().findIndex((track) => trackId(track) === trackId(playerState.currentTrack)));
  }
  createEffect(on(() => playlist$1(), (value) => {
    setPlaylist(value);
    updateCurrentlyPlayingIndex();
  }));
  createEffect(on(() => playerState.currentTrack, () => {
    updateCurrentlyPlayingIndex();
  }));
  return ssr(_tmpl$$3, ssrHydrationKey(), escape(createComponent$1(For, {
    get each() {
      return playlist2();
    },
    children: (track, index) => [trackId(playerState.currentTrack) === trackId(track) && ssr(_tmpl$3$1, ssrHydrationKey(), `/albums/${escape(track.albumId, true)}`, escape(track.album)), index() === (currentlyPlayingIndex() ?? 0) + 1 && ssr(_tmpl$4, ssrHydrationKey()), ssr(_tmpl$2$1, ssrHydrationKey(), `playlist-tracks-track${trackId(playerState.currentTrack) === trackId(track) ? " current" : ""}${trackId(playerState.currentTrack) === trackId(track) && playing() ? " playing" : ""}${index() < (currentlyPlayingIndex() ?? 0) ? " past" : ""}`, escape(createComponent$1(album, {
      album: track,
      size: 50,
      route: false
    })), index() === currentlyPlayingIndex() ? _tmpl$5[0] + ssrHydrationKey() + _tmpl$5[1] : _tmpl$6[0] + ssrHydrationKey() + _tmpl$6[1], escape(track.title), escape(track.artist), index() !== (currentlyPlayingIndex() ?? 0) && _tmpl$7[0] + ssrHydrationKey() + _tmpl$7[1])]
  })));
}

var _tmpl$$2 = ["<div", ' class="volume-container"><img class="adjust-volume-icon" src="/img/audio-white.svg" alt="Adjust Volume"><div class="volume-slider-container" style="', '"><div class="volume-slider-inner"><div class="volume-slider-background"></div><div class="volume-slider" style="', '"></div><div class="volume-slider-top" style="', '"></div></div></div></div>'];
let mouseEnterListener;
let mouseLeaveListener;
let dragStartListener;
let dragListener$1;
let dragEndListener$1;
let hideTimeout;
function eventToSeekPosition$1(element) {
  return 0;
}
function volumeRender() {
  let volumeContainerRef;
  let volumeSliderInnerRef;
  const [showVolume, setShowVolume] = createSignal(false);
  const [inside, setInside] = createSignal(false);
  const [sliderHeight, setSliderHeight] = createSignal(100);
  const [dragging, setDragging] = createSignal(false);
  const [applyDrag, setApplyDrag] = createSignal(false);
  function saveVolume(value) {
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
  createEffect(on(() => playerState.currentPlaybackSession?.volume ?? 1, (volume) => {
    const height = Math.round(Math.max(0, Math.min(100, volume * 100)));
    setSliderHeight(height);
  }));
  onMount(() => {
    if (isServer) {
      return;
    }
    function initiateClose() {
      hideTimeout = setTimeout(() => {
        setShowVolume(false);
        hideTimeout = void 0;
      }, 400);
    }
    mouseEnterListener = (_event) => {
      setInside(true);
      if (hideTimeout) {
        clearTimeout(hideTimeout);
        hideTimeout = void 0;
      }
      setShowVolume(true);
    };
    mouseLeaveListener = (_event) => {
      setInside(false);
      if (!dragging()) {
        initiateClose();
      }
    };
    dragStartListener = (event) => {
      if (event.button === 0) {
        setDragging(true);
        setApplyDrag(true);
        event.clientY;
        saveVolume(eventToSeekPosition$1());
      }
    };
    dragListener$1 = (event) => {
      if (!showVolume()) {
        return;
      }
      event.clientY;
      if (dragging()) {
        event.preventDefault();
        if (!applyDrag()) return;
      } else {
        return;
      }
      saveVolume(eventToSeekPosition$1());
    };
    dragEndListener$1 = (event) => {
      if (event.button === 0 && dragging()) {
        setDragging(false);
        if (!inside() && showVolume()) {
          initiateClose();
        }
        if (!applyDrag()) return;
        setApplyDrag(false);
        event.preventDefault();
      }
    };
    volumeContainerRef.addEventListener("mouseenter", mouseEnterListener);
    volumeContainerRef.addEventListener("mouseleave", mouseLeaveListener);
    volumeSliderInnerRef.addEventListener("mousedown", dragStartListener);
    window.addEventListener("mousemove", dragListener$1);
    window.addEventListener("mouseup", dragEndListener$1);
  });
  onCleanup(() => {
    if (isServer) {
      return;
    }
    volumeContainerRef.removeEventListener("mouseenter", mouseEnterListener);
    volumeContainerRef.removeEventListener("mouseleave", mouseLeaveListener);
    volumeSliderInnerRef.removeEventListener("mousedown", dragStartListener);
    window.removeEventListener("mousemove", dragListener$1);
    window.removeEventListener("mouseup", dragEndListener$1);
  });
  return ssr(_tmpl$$2, ssrHydrationKey(), "display:" + (showVolume() ? escape(void 0, true) : "none"), `height:${escape(sliderHeight(), true)}%`, `bottom:${escape(sliderHeight(), true)}%`);
}

var _tmpl$$1 = ["<div", ' class="visualization"><div class="visualization-media-controls-seeker-bar"><div class="visualization-media-controls-seeker-bar-progress-trigger visualization-media-controls-seeker-bar-visualizer" style="', '"><canvas class="visualization-canvas" width="0"', ' style="', '"></canvas></div><div class="visualization-media-controls-seeker-bar-progress-tooltip" style="', '">', "</div></div></div>"];
const VIZ_HEIGHT = 30;
const BAR_WIDTH = 2;
const BAR_GAP = 1;
const CURSOR_OFFSET = -1;
let visualizationData;
let mouseX;
let waitingForPlayback = true;
let targetPlaybackPos = 0;
function getTrackDuration$1() {
  return playerState.currentTrack?.duration ?? currentTrackLength();
}
function debounce(func) {
  let timer;
  return function(event) {
    if (timer) clearTimeout(timer);
    timer = setTimeout(func, 300, event);
  };
}
function eventToSeekPosition(element) {
  if (!element) return 0;
  const pos = element.getBoundingClientRect();
  const percentage = Math.min(100, Math.max(0, (mouseX - pos.left) / pos.width));
  return getTrackDuration$1() * percentage;
}
function seekTo(event) {
  const seekPos = Math.round(eventToSeekPosition(event.target));
  seek(seekPos, true);
  waitingForPlayback = true;
  targetPlaybackPos = seekPos;
}
let dragListener;
let dragEndListener;
let visibilityChangeListener;
let resizeListener;
let onSeekListener;
function player$1() {
  let canvas;
  let progressBarVisualizer;
  const [dragging, setDragging] = createSignal(false);
  const [applyDrag, setApplyDrag] = createSignal(false);
  const [seekPosition, setSeekPosition] = createSignal(currentSeek());
  const [playing$1, setPlaying] = createSignal(playing());
  const [data, setData] = createSignal([]);
  const [lastCursor, setLastCursor] = createSignal();
  const [lastDarken, setLastDarken] = createSignal(0);
  createComputed(() => {
    setPlaying(playerState.currentPlaybackSession?.playing ?? false);
  });
  function getSeekPosition() {
    return Math.max(Math.min(seekPosition() ?? 0, getTrackDuration$1()), 0);
  }
  function getCurrentSeekPosition() {
    return Math.max(Math.min(currentSeek() ?? 0, getTrackDuration$1()), 0);
  }
  function getProgressBarWidth() {
    if (applyDrag() && dragging()) {
      return getSeekPosition() / getTrackDuration$1();
    }
    return getCurrentSeekPosition() / getTrackDuration$1();
  }
  onMount(() => {
    if (!isServer) {
      dragListener = (event) => {
        mouseX = event.clientX;
        if (dragging()) {
          event.preventDefault();
          if (!applyDrag()) return;
        }
        setSeekPosition(eventToSeekPosition(progressBarVisualizer));
      };
      dragEndListener = (event) => {
        if (event.button === 0 && dragging()) {
          setDragging(false);
          if (!applyDrag()) return;
          setApplyDrag(false);
          seekTo(event);
          event.preventDefault();
        }
      };
      visibilityChangeListener = () => {
        if (document.visibilityState !== "hidden") {
          animationStart = void 0;
        }
      };
      resizeListener = debounce(async () => {
        if (playerState.currentTrack) {
          await loadVisualizationData(playerState.currentTrack);
          initVisualization();
        }
      });
      window.addEventListener("mousemove", dragListener);
      window.addEventListener("mouseup", dragEndListener);
      document.addEventListener("visibilitychange", visibilityChangeListener);
      window.addEventListener("resize", resizeListener);
      onSeekListener = (_seek, manual) => {
        if (manual) {
          if (!visualizationData) return;
          initVisualization();
        }
      };
      onSeek(onSeekListener);
    }
  });
  onCleanup(() => {
    if (!isServer) {
      window.removeEventListener("mousemove", dragListener);
      window.removeEventListener("mouseup", dragEndListener);
      document.removeEventListener("visibilitychange", visibilityChangeListener);
      window.removeEventListener("resize", resizeListener);
      offSeek(onSeekListener);
    }
  });
  createEffect(on(() => currentSeek(), (value) => {
    animationStart = document.timeline.currentTime;
    if (waitingForPlayback && (value ?? 0) > targetPlaybackPos && (targetPlaybackPos === 0 || (value ?? 0) <= targetPlaybackPos + 1) && playing$1()) {
      console.debug("playback started");
      waitingForPlayback = false;
      animationStart = void 0;
      startAnimation();
    }
  }));
  function initVisualization() {
    if (!visualizationData) {
      throw new Error("No visualizationData set");
    }
    setLastDarken(0);
    const ratio = window.devicePixelRatio;
    canvas.width = window.innerWidth * ratio;
    canvas.height = VIZ_HEIGHT * ratio;
    const ctx = canvas.getContext("2d");
    ctx.scale(ratio, ratio);
    ctx.fillStyle = "white";
    const delta = Math.max(1, visualizationData.length / window.innerWidth * 2);
    const sizedData = [];
    for (let i = 0; i < visualizationData.length && sizedData.length < window.innerWidth; i += delta) {
      sizedData.push(visualizationData[~~i]);
    }
    setData(sizedData);
    const cursor = getProgressBarWidth();
    ctx.fillStyle = "white";
    drawVisualizationPoints(0, data().length);
    darkenVisualization(cursor);
    drawVisualization(cursor);
  }
  function darkenVisualization(cursor) {
    const ctx = canvas.getContext("2d");
    const lastDarkenValue = lastDarken();
    const points = data();
    const darken = ~~(cursor * points.length);
    if (lastDarkenValue > darken) {
      ctx.fillStyle = "white";
      drawVisualizationPoints(darken + 1, lastDarkenValue);
    }
    ctx.fillStyle = "#222222";
    drawVisualizationPoints(Math.max(0, lastDarkenValue - 1), darken + 1);
    setLastDarken(darken);
  }
  function clearVisualization(cursor) {
    const ctx = canvas.getContext("2d");
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    drawVisualization(cursor);
  }
  function drawVisualization(cursor) {
    const ctx = canvas.getContext("2d");
    const ratio = window.devicePixelRatio;
    const lastCursorValue = lastCursor();
    if (typeof lastCursorValue === "number") {
      const start = lastCursorValue;
      const end = lastCursorValue;
      const paintStart = ~~(start * canvas.width / ratio) - 1;
      const paintEnd = Math.ceil(end * canvas.width / ratio) + 3;
      ctx.clearRect(paintStart, 0, paintEnd - paintStart, canvas.height);
      const points = data();
      const paintStartI = ~~(start * points.length) - 2;
      const paintEndI = Math.ceil(end * points.length) + 2;
      darkenVisualization(cursor);
      const lastDarkenValue = lastDarken();
      ctx.fillStyle = "white";
      drawVisualizationPoints(Math.max(lastDarkenValue + 1, paintStartI), paintEndI);
    } else {
      darkenVisualization(cursor);
    }
    ctx.fillStyle = "white";
    ctx.fillRect(canvas.width * cursor / ratio + CURSOR_OFFSET, 0, 2, VIZ_HEIGHT);
    setLastCursor(cursor);
  }
  function drawVisualizationPoints(start, end) {
    if (start >= end) return;
    const ctx = canvas.getContext("2d");
    const points = data();
    ctx.clearRect(start * (BAR_GAP + BAR_WIDTH) - 0.5 + CURSOR_OFFSET, 0, (end - start) * (BAR_GAP + BAR_WIDTH) - 0.5, VIZ_HEIGHT);
    for (let i = start; i < end; i++) {
      const point = points[i];
      ctx.fillRect(i * (BAR_GAP + BAR_WIDTH) + CURSOR_OFFSET, VIZ_HEIGHT / 2 - point / 2, BAR_WIDTH / 2, point);
    }
  }
  async function loadVisualizationData(track) {
    const max = window.innerWidth / (BAR_GAP + BAR_WIDTH);
    const data2 = await api.getTrackVisualization(track, track.type, max);
    data2.forEach((x, i) => {
      data2[i] = Math.max(3, Math.round(x / 255 * VIZ_HEIGHT));
    });
    visualizationData = data2;
  }
  createEffect(on(() => playerState.currentTrack, async (track, prev) => {
    if (prev && track && trackId(prev) === trackId(track)) return;
    visualizationData = void 0;
    setData([]);
    const cursor = getProgressBarWidth();
    clearVisualization(cursor);
    waitingForPlayback = true;
    targetPlaybackPos = 0;
    if (track) {
      await loadVisualizationData(track);
      initVisualization();
    }
  }));
  createEffect(on(() => currentTrackLength(), () => {
    setSeekPosition(eventToSeekPosition(progressBarVisualizer));
    updateVisualizationBarOpacity();
  }));
  createEffect(on(() => playing$1(), (playing2) => {
    if (!playing2) {
      waitingForPlayback = true;
    }
    if (dragging()) {
      setApplyDrag(false);
    }
  }));
  let nextTrackListener;
  let previousTrackListener;
  onCleanup(() => {
    offNextTrack(nextTrackListener);
    offPreviousTrack(previousTrackListener);
  });
  createSignal("NONE" /* none */);
  onMount(() => {
    if (isServer) return;
  });
  onCleanup(() => {
    if (isServer) return;
  });
  let animationStart;
  function progressAnimationFrame(ts) {
    if (!animationStart) animationStart = ts;
    const elapsed = ts - animationStart;
    const duration = getTrackDuration$1();
    if (typeof currentSeek() !== "undefined" && typeof duration !== "undefined") {
      const offset = elapsed / 1e3 * (1 / duration);
      updateVisualizationBarOpacity(offset);
    }
    if (!playing$1() || waitingForPlayback) {
      animationStart = void 0;
      console.debug("Stopping animation");
      return;
    }
    window.requestAnimationFrame(progressAnimationFrame);
  }
  function startAnimation() {
    window.requestAnimationFrame((ts) => {
      animationStart = ts;
      window.requestAnimationFrame(progressAnimationFrame);
    });
  }
  function updateVisualizationBarOpacity(offset = 0) {
    if (waitingForPlayback) {
      return;
    }
    const cursor = getProgressBarWidth() + offset;
    drawVisualization(cursor);
  }
  return ssr(_tmpl$$1, ssrHydrationKey(), `top:-${escape(Math.round(VIZ_HEIGHT / 2), true) - 2}px;height:${escape(VIZ_HEIGHT, true)}px`, ssrAttribute("height", escape(VIZ_HEIGHT, true), false), `height:${escape(VIZ_HEIGHT, true)}px`, `left:max(30px, min(100vw - 40px, ${escape(getSeekPosition(), true) / escape(getTrackDuration$1(), true) * 100}%))` + (";display:" + (applyDrag() && dragging() ? "block" : escape(void 0, true))), escape(toTime(Math.round(getSeekPosition()))));
}

var _tmpl$ = ["<div", ' class="player"><!--$-->', '<!--/--><div class="player-controls"><div class="player-now-playing"><div class="player-album-details">', '</div></div><div class="player-media-controls"><div class="player-media-controls-track"><button class="media-button button"><img class="previous-track-button" src="/img/next-button-white.svg" alt="Previous Track"></button><button class="media-button button" style="', '"><img class="pause-button" src="/img/pause-button-white.svg" alt="Pause"></button><button class="media-button button" style="', '"><img class="play-button" src="/img/play-button-white.svg" alt="Play"></button><button class="media-button button"><img class="next-track-button" src="/img/next-button-white.svg" alt="Next Track"></button><img class="show-playback-quality-icon" src="/img/more-options-white.svg" alt="Show Playback Quality"></div><div class="player-media-controls-seeker"><span class="player-media-controls-seeker-current-time">', '</span>//<span class="player-media-controls-seeker-total-time">', '</span></div></div><div class="player-track-options"><div class="player-track-options-buttons"><!--$-->', '<!--/--><img class="show-audio-zones-icon" src="/img/speaker-white.svg" alt="Configure Audio Outputs"><img class="show-playback-sessions-icon" src="/img/speaker-white.svg" alt="Show Playback Sessions"><img class="show-playlist-icon" src="/img/playlist-white.svg" alt="Show Playlist"></div><div class="player-track-options-mobile"><img class="mobile-playback-options" src="/img/more-options-white.svg" alt="Show Playback Options"><img class="show-playlist-icon" src="/img/playlist-white.svg" alt="Show Playlist"></div></div></div><div class="', '"><!--$-->', '<!--/--><img class="show-audio-zones-icon" src="/img/speaker-white.svg" alt="Configure Audio Outputs"><img class="show-playback-sessions-icon" src="/img/speaker-white.svg" alt="Show Playback Sessions"><img class="show-playback-quality-icon" src="/img/more-options-white.svg" alt="Show Playback Quality"></div><div class="playlist-slideout" style="', '"><div class="playlist-slideout-content">', '</div><div class="playlist-slideout-back-to-now-playing-top">Back to now playing</div><div class="playlist-slideout-back-to-now-playing-bottom">Back to now playing</div></div></div>'], _tmpl$2 = ["<div", ' class="player-album-details-icon">', "</div>"], _tmpl$3 = ["<div", ' class="player-now-playing-details"><div class="player-now-playing-details-title"><a', ">", '</a></div><div class="player-now-playing-details-artist"><a', ">", '</a></div><div class="player-now-playing-details-album">Playing from: <a', ">", "</a></div></div>"];
function getTrackDuration() {
  return playerState.currentTrack?.duration ?? currentTrackLength();
}
function player() {
  let playlistSlideout;
  const [showingPlaylist, setShowingPlaylist] = createSignal(false);
  const [playing$1, setPlaying] = createSignal(playing());
  const [showTrackOptionsMobile, setShowTrackOptionsMobile] = createSignal(false);
  clientSignal(showAudioZones);
  clientSignal(showPlaybackSessions);
  clientSignal(showPlaybackQuality);
  createComputed(() => {
    setPlaying(playerState.currentPlaybackSession?.playing ?? false);
  });
  function closePlaylist() {
    if (!showingPlaylist()) return;
    setShowingPlaylist(false);
    setTimeout(() => {
      playlistSlideout.style.display = "none";
    }, 200);
  }
  createEffect(on(() => location.pathname, () => {
    closePlaylist();
  }));
  const handleClick = (event) => {
    {
      closePlaylist();
    }
  };
  onMount(() => {
    if (isServer) return;
    window.addEventListener("click", handleClick);
  });
  onCleanup(() => {
    if (isServer) return;
    window.removeEventListener("click", handleClick);
  });
  let nextTrackListener;
  let previousTrackListener;
  onMount(() => {
    onNextTrack(nextTrackListener = () => {
      if (!showingPlaylist()) return;
    });
    onPreviousTrack(previousTrackListener = () => {
      if (!showingPlaylist()) return;
    });
  });
  onCleanup(() => {
    offNextTrack(nextTrackListener);
    offPreviousTrack(previousTrackListener);
  });
  createSignal("NONE" /* none */);
  onMount(() => {
    if (isServer) return;
  });
  onCleanup(() => {
    if (isServer) return;
  });
  return ssr(_tmpl$, ssrHydrationKey(), escape(createComponent$1(player$1, {})), escape(createComponent$1(Show, {
    get when() {
      return playerState.currentTrack;
    },
    children: (currentTrack) => [ssr(_tmpl$2, ssrHydrationKey(), escape(createComponent$1(album, {
      get album() {
        return currentTrack();
      },
      size: 70,
      artist: false,
      title: false
    }))), ssr(_tmpl$3, ssrHydrationKey(), ssrAttribute("href", escape(albumRoute(currentTrack()), true), false) + ssrAttribute("title", escape(currentTrack().title, true), false), escape(currentTrack().title), ssrAttribute("href", escape(artistRoute(currentTrack()), true), false) + ssrAttribute("title", escape(currentTrack().artist, true), false), escape(currentTrack().artist), ssrAttribute("href", escape(albumRoute(currentTrack()), true), false) + ssrAttribute("title", escape(currentTrack().album, true), false), escape(currentTrack().album))]
  })), "display:" + (playing$1() ? "initial" : "none"), "display:" + (!playing$1() ? "initial" : "none"), escape(toTime(currentSeek() ?? 0)), escape(toTime(getTrackDuration())), escape(createComponent$1(volumeRender, {})), `player-track-options-mobile-buttons${showTrackOptionsMobile() ? " visible" : " hidden"}`, escape(createComponent$1(volumeRender, {})), `transform:translateX(${showingPlaylist() ? 0 : 100}%)`, escape(createComponent$1(playlist, {})));
}

const $$Astro = createAstro();
const $$Layout = createComponent(($$result, $$props, $$slots) => {
  const Astro2 = $$result.createAstro($$Astro, $$props, $$slots);
  Astro2.self = $$Layout;
  const { title } = Astro2.props;
  return renderTemplate`<html lang="en"> <head><meta charset="UTF-8"><meta name="description" content="Astro description"><meta name="viewport" content="width=device-width"><meta name="turbo-cache-control" content="no-cache"><link rel="icon" type="image/ico" href="/favicon.ico"><meta name="generator"${addAttribute(Astro2.generator, "content")}><title>${title}</title>${renderHead()}</head> <body> <div id="root" class="dark"> <header> ${renderComponent($$result, "ScanStatusBanner", null, { "client:only": true, "client:component-hydration": "only", "client:component-path": "~/components/ScanStatusBanner", "client:component-export": "default" })} </header> <section class="navigation-bar-and-main-content"> ${renderComponent($$result, "Aside", $$Aside, {})} <main class="main-content"> ${renderComponent($$result, "Search", null, { "client:only": true, "client:component-hydration": "only", "client:component-path": "~/components/Search", "client:component-export": "default" })} ${renderSlot($$result, $$slots["default"])} ${renderComponent($$result, "PlaybackQualityModal", null, { "client:only": true, "client:component-hydration": "only", "client:component-path": "~/components/PlaybackQualityModal", "client:component-export": "default" })} ${renderComponent($$result, "PlaybackSessionsModal", null, { "client:only": true, "client:component-hydration": "only", "client:component-path": "~/components/PlaybackSessionsModal", "client:component-export": "default" })} ${renderComponent($$result, "AudioZonesModal", null, { "client:only": true, "client:component-hydration": "only", "client:component-path": "~/components/AudioZonesModal", "client:component-export": "default" })} ${renderComponent($$result, "ChangePlaybackTargetModal", null, { "client:only": true, "client:component-hydration": "only", "client:component-path": "~/components/ChangePlaybackTargetModal", "client:component-export": "default" })} </main> </section> <footer data-turbo-permanent id="footer" class="footer-player-footer"> <div class="footer-player-container"> <div class="footer-player"> ${renderComponent($$result, "Player", player, { "client:load": true, "client:component-hydration": "load", "client:component-path": "~/components/Player", "client:component-export": "default" })} </div> </div> </footer> </div> </body></html>`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/layouts/Layout.astro", void 0);

export { $$Layout as $, connections as a, connection as b, clientSignal as c, connectionName as d };
//# sourceMappingURL=Layout_aLm9q08O.mjs.map
