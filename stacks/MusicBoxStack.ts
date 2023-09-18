import { SSMClient } from "@aws-sdk/client-ssm";
import { StackContext, Api } from "sst/constructs";
import { fetchSstSecret } from "../sst-secrets";

export async function API({ app, stack }: StackContext) {
    const ssm = new SSMClient({ region: stack.region });

    const api = new Api(stack, "menu", {
        defaults: {
            function: {
                runtime: "rust",
                environment: {
                    PROXY_HOST: await fetchSstSecret(
                        ssm,
                        app.name,
                        "PROXY_HOST",
                        app.stage
                    ),
                },
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
