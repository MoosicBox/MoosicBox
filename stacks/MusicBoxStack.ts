import { StackContext, Api } from "sst/constructs";

export function API({ stack }: StackContext) {
    const api = new Api(stack, "api", {
        defaults: {
            function: {
                runtime: "rust",
            },
        },
        routes: {
            "GET /albums": "packages/menu/src/moosicbox_menu.handler",
        },
    });

    stack.addOutputs({
        ApiEndpoint: api.url,
    });
}
