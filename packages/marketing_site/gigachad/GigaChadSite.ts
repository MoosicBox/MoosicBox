import { ComponentResourceOptions } from '@pulumi/pulumi';
import { spawnSync } from 'child_process';

const NO_CACHE_POLICY_ID = '4135ea2d-6df8-44a3-9df3-4b5a84be39ad';

export function createGigaChadSite(
    name: string,
    args: sst.aws.StaticSiteArgs = {},
    opts: ComponentResourceOptions = {},
) {
    args.indexPage = args.indexPage ?? 'index';
    args.build = args.build ?? {
        command:
            'cargo run --bin moosicbox_marketing_site --no-default-features --features htmx,static-routes,assets gen',
        output: 'gen',
    };

    const dynamicRoutes = getDynamicRoutes();

    console.log('Using dynamic route paths:', dynamicRoutes);

    const apiName = `${name}-api`;

    const api = new sst.aws.ApiGatewayV2(apiName, {
        transform: {
            route: {
                handler: {
                    runtime: 'rust' as 'go', // FIXME: remove this cast once rust is a valid runtime
                    transform: {
                        function: {
                            runtime: 'provided.al2023',
                            timeout: 300,
                        },
                    },
                },
            },
        },
    });

    dynamicRoutes.forEach((route) => {
        api.route(`GET ${route}`, {
            handler: 'src/moosicbox_marketing_site_lambda.handler',
            runtime: 'rust' as 'go', // FIXME: remove this cast once rust is a valid runtime
        });
    });

    const staticSiteName = `${name}-static`;

    const staticSite = new sst.aws.StaticSite(
        staticSiteName,
        {
            transform: {
                ...args.transform,
                cdn: (args) => {
                    args.origins = $output(args.origins).apply((origins) => [
                        ...origins,
                        {
                            originId: 'api',
                            domainName: api.url.apply(
                                (url) => new URL(url!).host,
                            ),
                            customOriginConfig: {
                                httpPort: 80,
                                httpsPort: 443,
                                originProtocolPolicy: 'https-only',
                                originSslProtocols: ['TLSv1.2'],
                            },
                        },
                    ]);

                    args.orderedCacheBehaviors = dynamicRoutes.map((route) => {
                        return {
                            pathPattern: route,
                            targetOriginId: 'api',
                            allowedMethods: ['GET', 'HEAD', 'OPTIONS'],
                            cachedMethods: ['GET', 'HEAD'],
                            viewerProtocolPolicy: 'redirect-to-https',
                            cachePolicyId: NO_CACHE_POLICY_ID,
                        };
                    });
                },
            },
            ...args,
        },
        opts,
    );

    return {
        api,
        staticSite,
        linkable: new sst.Linkable(name, {
            properties: {},
        }),
    };
}

function getDynamicRoutes() {
    const { status, stdout, stderr } = spawnSync(
        'cargo',
        [
            'run',
            '--bin',
            'moosicbox_marketing_site',
            '--no-default-features',
            '--features',
            'htmx',
            'dynamic-routes',
        ],
        {
            encoding: 'utf8',
        },
    );

    if (status !== 0) {
        if (stderr.length > 0) {
            console.error(stderr);
        }
        throw new Error('Failed to get dynamic routes');
    }

    return stdout
        .split(/\s/g)
        .map((x) => x.trim())
        .filter((x) => x.length > 0);
}
