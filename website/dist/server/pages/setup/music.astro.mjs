import { c as createComponent, r as renderTemplate, d as renderComponent, m as maybeRenderHead } from '../../chunks/astro/server_BHkv7Nwt.mjs';
import { $ as $$Layout } from '../../chunks/Layout_BRzU7R80.mjs';
/* empty css                                    */
export { renderers } from '../../renderers.mjs';

const $$Music = createComponent(($$result, $$props, $$slots) => {
  return renderTemplate`${renderComponent($$result, "Layout", $$Layout, { "title": "Hello | MoosicBox" }, { "default": ($$result2) => renderTemplate` ${maybeRenderHead()}<p>Where do you store your music?</p> <button type="button" class="remove-button-styles add-music-folder-button">
Add Folder
</button> ` })}`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/setup/music.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/setup/music.astro";
const $$url = "/setup/music";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
    __proto__: null,
    default: $$Music,
    file: $$file,
    url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
//# sourceMappingURL=music.astro.mjs.map
