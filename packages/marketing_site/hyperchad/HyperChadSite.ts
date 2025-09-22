import { ComponentResourceOptions } from '@pulumi/pulumi';
import { spawnSync } from 'child_process';

const NO_CACHE_POLICY_ID = '4135ea2d-6df8-44a3-9df3-4b5a84be39ad';
const ALL_VIEWER_EXCEPT_HOST_POLICY_ID = 'b689b0a8-53d0-40ab-baf2-68738e2966ac';

const RENDERERS = {
    vanillaJs: {
        feature: 'vanilla-js',
        bin: 'moosicbox_marketing_site_lambda_vanilla_js',
    },
} as const;

export function createHyperChadSite(
    name: string,
    renderer: keyof typeof RENDERERS,
    args: sst.aws.StaticSiteArgs = {},
    opts: ComponentResourceOptions = {},
) {
    args.indexPage = args.indexPage ?? 'index';
    args.build = args.build ?? {
        command: `cargo run --release --bin moosicbox_marketing_site --no-default-features --features ${RENDERERS[renderer].feature},static-routes,assets gen`,
        output: 'gen',
    };

    const dynamicRoutes = getDynamicRoutes(renderer);

    console.log('Using dynamic route paths:', dynamicRoutes);

    const lambdaName = `${name}-lambda`;
    const rustLog = process.env.LAMBDA_RUST_LOG ?? '';

    const lambda = new sst.aws.Function(lambdaName, {
        handler: `src/${RENDERERS[renderer].bin}.handler`,
        runtime: 'rust' as 'go', // FIXME: remove this cast once rust is a valid runtime
        timeout: '5 minutes',
        environment: { RUST_LOG: rustLog },
        transform: {
            function: {
                runtime: 'provided.al2023',
            },
        },
        streaming: true,
        url: {
            cors: {
                allowCredentials: false,
                allowHeaders: ['*'],
                allowMethods: ['GET', 'POST', 'PUT', 'DELETE', 'HEAD', 'PATCH'],
                allowOrigins: ['*'],
                exposeHeaders: ['*'],
                maxAge: '1 day',
            },
        },
    });

    const staticSiteName = `${name}-static`;

    const staticSite = new sst.aws.StaticSite(
        staticSiteName,
        {
            assets: {
                fileOptions: [
                    {
                        files: ['**/*'],
                        cacheControl: 'max-age=31536000,public,immutable',
                    },
                ],
            },
            transform: {
                ...args.transform,
                cdn: (args) => {
                    args.origins = $output(args.origins).apply((origins) => [
                        ...origins,
                        {
                            originId: 'api',
                            domainName: lambda.url.apply(
                                (url: string) => new URL(url!).host,
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
                            originRequestPolicyId:
                                ALL_VIEWER_EXCEPT_HOST_POLICY_ID,
                        };
                    });
                },
            },
            ...args,
        },
        opts,
    );

    return {
        lambda,
        staticSite,
        linkable: new sst.Linkable(name, {
            properties: {},
        }),
    };
}

function getDynamicRoutes(renderer: keyof typeof RENDERERS) {
    const { status, stdout, stderr } = spawnSync(
        'cargo',
        [
            'run',
            '--release',
            '--bin',
            'moosicbox_marketing_site',
            '--no-default-features',
            '--features',
            RENDERERS[renderer].feature,
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
