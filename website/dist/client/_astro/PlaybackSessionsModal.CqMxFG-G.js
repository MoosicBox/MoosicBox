import{d as z,c as M,h as ts,g as n,i as a,b as y,a as $,e as _,I as ns,f as cs,k as rs,r as A,F as os,t as c,l as ds}from"./web.o2PnP_jd.js";import{p as k,D as T,E as D,F as G,w as S,G as ks,C as ps,H as bs}from"./ChangePlaybackTargetModal.BU1Vz2Zf.js";import{c as $s}from"./Album.B7ZnFso0.js";import{p as H,c as ys}from"./api.DZqBGp_p.js";/* empty css                        */var us=c("<div class=playback-sessions><div class=playback-sessions-list>"),gs=c('<div><div class=playback-sessions-list-session-header><img class=playback-sessions-list-session-header-speaker-icon src=/img/speaker-white.svg><h2 class=playback-sessions-list-session-header-session-name></h2><h3 class=playback-sessions-list-session-header-session-tracks-queued><!$><!/> track<!$><!/> queued</h3><!$><!/><!$><!/><div class=playback-sessions-list-session-header-right><div class=playback-sessions-list-session-header-delete-session><img class=trash-icon src=/img/trash-white.svg alt="Delete playback session"></div></div></div><div class=playback-sessions-playlist-tracks-container><div class=playback-sessions-playlist-tracks></div><!$><!/>'),ms=c("<img class=playback-sessions-list-session-header-checkmark-icon src=/img/checkmark-white.svg>"),vs=c("<img class=playback-sessions-list-session-header-playing-icon src=/img/audio-white.svg>"),hs=c("<div class=playback-sessions-playlist-tracks-track><div class=playback-sessions-playlist-tracks-track-album-artwork><div class=playback-sessions-playlist-tracks-track-album-artwork-icon></div></div><div class=playback-sessions-playlist-tracks-track-details><div class=playback-sessions-playlist-tracks-track-details-title></div><div class=playback-sessions-playlist-tracks-track-details-artist>"),_s=c("<div class=playback-sessions-playlist-tracks-overlay>");const Z={};function Ss(){const[p,u]=M(k.playbackSessions),[b,g]=M();ts(()=>{u(k.playbackSessions),b()&&g(p().find(s=>s.sessionId===b()?.sessionId))});function m(s){s===k.currentPlaybackSession?.sessionId&&T(H(i=>{i.playbackSessions.find(l=>l.sessionId===s),u(p().filter(l=>l.sessionId!==s));const e=p()[0];e&&D(i,e,!0)})),G.deleteSession(s)}function h(s){s.sessionId!==k.currentPlaybackSession?.sessionId&&T(H(i=>{D(i,s,!0)}))}function r(s){const i=Z[s.sessionId];if(i?.position===s.position&&i?.tracks.every((l,o)=>{const d=s.playlist.tracks[o];if(!d){console.error("Failed to queue tracks");return}return"trackId"in d&&"trackId"in l&&d.trackId===l?.trackId||"id"in d&&"id"in l&&d.id===l?.id}))return i.tracks;const e=s.playlist.tracks.slice(s.position??0,s.playlist.tracks.length);return Z[s.sessionId]={position:s.position,tracks:e},e}return(()=>{var s=n(us),i=s.firstChild;return a(i,y(os,{get each(){return k.playbackSessions},children:e=>(()=>{var l=n(gs),o=l.firstChild,d=o.firstChild,f=d.nextSibling,v=f.nextSibling,R=v.firstChild,[x,j]=$(R.nextSibling),B=x.nextSibling,J=B.nextSibling,[I,K]=$(J.nextSibling);I.nextSibling;var L=v.nextSibling,[w,O]=$(L.nextSibling),Q=w.nextSibling,[C,U]=$(Q.nextSibling),V=C.nextSibling,W=V.firstChild,P=o.nextSibling,F=P.firstChild,X=F.nextSibling,[Y,ss]=$(X.nextSibling);return o.$$click=()=>h(e),a(f,()=>e.name),a(v,()=>r(e).length,x,j),a(v,()=>r(e).length===1?"":"s",I,K),a(o,(()=>{var t=_(()=>k.currentPlaybackSession?.sessionId===e.sessionId);return()=>t()&&n(ms)})(),w,O),a(o,(()=>{var t=_(()=>!!e.playing);return()=>t()&&n(vs)})(),C,U),W.$$click=t=>{m(e.sessionId),t.stopImmediatePropagation()},a(F,y(ns,{get each(){return r(e)},children:(t,es)=>es>=4?[]:(()=>{var N=n(hs),q=N.firstChild,as=q.firstChild,is=q.nextSibling,E=is.firstChild,ls=E.nextSibling;return a(as,y($s,{get album(){return t()},size:40,route:!1})),a(E,()=>t().title),a(ls,()=>t().artist),N})()})),a(P,(()=>{var t=_(()=>r(e).length>=3);return()=>t()&&n(_s)})(),Y,ss),cs(()=>rs(l,`playback-sessions-list-session${k.currentPlaybackSession?.sessionId===e.sessionId?" active":""}`)),A(),l})()})),s})()}z(["click"]);var fs=c('<div class=playback-sessions-modal-container><div class=playback-sessions-modal-header><h1>Playback Sessions</h1><button class=playback-sessions-modal-header-new-button>New</button><div class=playback-sessions-modal-close><img class=cross-icon src=/img/cross-white.svg alt="Close playlist sessions modal"></div></div><div class=playback-sessions-modal-content>'),xs=c("<div data-turbo-permanent id=playback-sessions-modal>");function Ns(){ds(async()=>{await ks()});const[p]=ys(S);function u(){G.createSession({name:"New Session",playlist:{tracks:[]},playbackTarget:bs()})}return(()=>{var b=n(xs);return a(b,y(ps,{show:()=>p(),onClose:()=>S.set(!1),get children(){var g=n(fs),m=g.firstChild,h=m.firstChild,r=h.nextSibling,s=r.nextSibling,i=m.nextSibling;return r.$$click=()=>u(),s.$$click=e=>{S.set(!1),e.stopImmediatePropagation()},a(i,y(Ss,{})),A(),g}})),b})()}z(["click"]);export{Ns as default};
//# sourceMappingURL=PlaybackSessionsModal.CqMxFG-G.js.map