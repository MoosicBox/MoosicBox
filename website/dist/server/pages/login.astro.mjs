import { c as createComponent, r as renderTemplate, d as renderComponent } from '../chunks/astro/server_BHkv7Nwt.mjs';
import { $ as $$Layout } from '../chunks/Layout_aLm9q08O.mjs';
export { renderers } from '../renderers.mjs';

const $$Login = createComponent(($$result, $$props, $$slots) => {
  return renderTemplate`${renderComponent($$result, "Layout", $$Layout, { "title": "Login | MoosicBox" }, { "default": ($$result2) => renderTemplate` Login ` })}`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/login.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/login.astro";
const $$url = "/login";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
	__proto__: null,
	default: $$Login,
	file: $$file,
	url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
//# sourceMappingURL=login.astro.mjs.map
