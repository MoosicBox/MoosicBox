import { c as createComponent$1, r as renderTemplate, d as renderComponent } from '../chunks/astro/server_BHkv7Nwt.mjs';
import { c as clientSignal, a as connections, b as connection, d as connectionName, $ as $$Layout } from '../chunks/Layout_aLm9q08O.mjs';
import { ssr, ssrHydrationKey, ssrAttribute, escape, createComponent } from 'solid-js/web';
import { createSignal, Show, For } from 'solid-js';
import '../chunks/settings.54b818e5_l0sNRNKZ.mjs';
export { renderers } from '../renderers.mjs';

var _tmpl$ = ["<div", '><section><ul><li>Name: <input type="text"', "><button>save</button></li></ul><!--$-->", '<!--/--><button type="button">New connection</button><ul><li>Name: <input type="text"', '><button>save</button></li><li>API Url: <input type="text"', '><button>save</button></li><li>Client ID: <input type="text"', '><button>save</button></li><li>Token: <input type="text"', '><button>save</button></li><li>Static Token: <input type="text"', '><button>save</button></li><li>Magic Token: <input type="text"><button>save</button></li></ul><!--$-->', "<!--/--><!--$-->", '<!--/--></section><hr><section><button type="button" class="remove-button-styles moosicbox-button">Scan</button></section></div>'], _tmpl$2 = ["<select", ' name="connections" id="connections-dropdown">', "</select>"], _tmpl$3 = ["<option", "", ">", "</option>"];
function settingsPage() {
  const [$connections, _setConnections] = clientSignal(connections);
  const [$connection, setConnection] = clientSignal(connection);
  const [$connectionName, setConnectionName] = clientSignal(connectionName);
  const [status, setStatus] = createSignal();
  const [loading, setLoading] = createSignal(false);
  return ssr(_tmpl$, ssrHydrationKey(), ssrAttribute("value", escape($connectionName(), true), false), escape(createComponent(Show, {
    get when() {
      return $connections();
    },
    children: (connections2) => ssr(_tmpl$2, ssrHydrationKey(), escape(createComponent(For, {
      get each() {
        return connections2();
      },
      children: (con) => ssr(_tmpl$3, ssrHydrationKey() + ssrAttribute("value", escape(con.id, true), false), ssrAttribute("selected", con.id === $connection()?.id, true), escape(con.name))
    })))
  })), ssrAttribute("value", escape($connection()?.name, true) ?? "New connection", false), ssrAttribute("value", escape($connection()?.apiUrl, true) ?? "", false), ssrAttribute("value", escape($connection()?.clientId, true) ?? "", false), ssrAttribute("value", escape($connection()?.token, true) ?? "", false), ssrAttribute("value", escape($connection()?.staticToken, true) ?? "", false), status() && escape(status()), loading() && "loading...");
}

const $$Settings = createComponent$1(($$result, $$props, $$slots) => {
  return renderTemplate`${renderComponent($$result, "Layout", $$Layout, { "title": "MoosicBox" }, { "default": ($$result2) => renderTemplate` ${renderComponent($$result2, "SettingsPage", settingsPage, { "client:load": true, "client:component-hydration": "load", "client:component-path": "~/routes/settings.tsx", "client:component-export": "default" })} ` })}`;
}, "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/settings.astro", void 0);

const $$file = "/home/bsteffaniak/GitHub/MoosicBoxUI/src/pages/settings.astro";
const $$url = "/settings";

const _page = /*#__PURE__*/Object.freeze(/*#__PURE__*/Object.defineProperty({
    __proto__: null,
    default: $$Settings,
    file: $$file,
    url: $$url
}, Symbol.toStringTag, { value: 'Module' }));

const page = () => _page;

export { page };
//# sourceMappingURL=settings.astro.mjs.map
