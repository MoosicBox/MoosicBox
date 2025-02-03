import { ComponentResourceOptions } from '@pulumi/pulumi';
import { spawnSync } from 'child_process';

const NO_CACHE_POLICY_ID = '4135ea2d-6df8-44a3-9df3-4b5a84be39ad';

export function createGigaChadSite(
    name: string,
    args: sst.aws.StaticSiteArgs = {},
    opts: ComponentResourceOptions = {},
) {
    args.build = {
        command:
            'cargo run --no-default-features --features htmx,static-routes gen',
        output: 'gen',
    };

    const dynamicRoutes = getDynamicRoutes();

    buildServer();

    console.log('Using dynamic route paths:', dynamicRoutes);

    args.indexPage = args.indexPage ?? 'index';

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
                            domainName: origins[0].domainName,
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
        staticSite,
        linkable: new sst.Linkable(name, { properties: {} }),
    };
}

function getDynamicRoutes() {
    const { status, stdout, stderr } = spawnSync(
        'cargo',
        [
            'run',
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

function buildServer() {
    const { status, stderr } = spawnSync(
        'cargo',
        [
            'build',
            '--release',
            '--no-default-features',
            '--features',
            'htmx,lambda',
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

    console.log('Successfully built release server');
}
