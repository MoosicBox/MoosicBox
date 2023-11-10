import { SSMClient } from '@aws-sdk/client-ssm';
import { StackContext, Api, WebSocketApi, Stack } from 'sst/constructs';
import { fetchSstSecret } from '../sst-secrets';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
// eslint-disable-next-line import/extensions
import {
    Distribution,
    DistributionDomainProps,
} from 'sst/constructs/Distribution.js';
import {
    AllowedMethods,
    CachePolicy,
    HttpVersion,
    OriginRequestPolicy,
    ResponseHeadersPolicy,
    ViewerProtocolPolicy,
} from 'aws-cdk-lib/aws-cloudfront';

const domainSlug = 'api';
const domain = 'moosicbox.com';
const defaultStageName = 'prod';

function getCustomDomain(stack: Stack): DistributionDomainProps {
    return {
        domainName:
            stack.stage === defaultStageName
                ? `${domainSlug}.${domain}`
                : `${domainSlug}-${stack.stage}.${domain}`,
        hostedZone: domain,
    };
}

export async function API({ app, stack }: StackContext) {
    const ssm = new SSMClient({ region: stack.region });

    const api = new Api(stack, 'api', {
        defaults: {
            function: {
                runtime: 'rust',
                timeout: '5 minutes',
                environment: {
                    PROXY_HOST: await fetchSstSecret(
                        ssm,
                        app.name,
                        'PROXY_HOST',
                        app.stage,
                    ),
                },
            },
        },
        routes: {
            'GET /albums': 'packages/menu/src/moosicbox_menu.handler',
            'GET /track': 'packages/files/src/moosicbox_files.handler',
        },
    });

    const websocketApi = new WebSocketApi(stack, 'websockets', {
        defaults: {
            function: {
                runtime: 'rust',
                timeout: '5 minutes',
                environment: {
                    PROXY_HOST: await fetchSstSecret(
                        ssm,
                        app.name,
                        'PROXY_HOST',
                        app.stage,
                    ),
                },
            },
        },
        routes: {
            $connect: 'packages/ws/src/moosicbox_ws.handler',
            $default: 'packages/ws/src/moosicbox_ws.handler',
            $disconnect: 'packages/ws/src/moosicbox_ws.handler',
            sendMessage: 'packages/ws/src/moosicbox_ws.handler',
        },
        cdk: {
            webSocketStage: {
                stageName: 'ws',
            },
        },
    });

    const apiDomainName = `${api.cdk.httpApi.httpApiId}.execute-api.${stack.region}.amazonaws.com`;
    const websocketApiDomainName = `${websocketApi.cdk.webSocketApi.apiId}.execute-api.${stack.region}.amazonaws.com`;
    const customDomain = getCustomDomain(stack);

    new Distribution(stack, 'API-Proxy-Distribution', {
        customDomain,
        cdk: {
            distribution: {
                defaultBehavior: {
                    origin: new origins.HttpOrigin(apiDomainName),
                    cachePolicy: CachePolicy.CACHING_DISABLED,
                    originRequestPolicy:
                        OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                    responseHeadersPolicy:
                        ResponseHeadersPolicy.CORS_ALLOW_ALL_ORIGINS,
                    allowedMethods: AllowedMethods.ALLOW_ALL,
                    viewerProtocolPolicy:
                        ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
                },
                additionalBehaviors: {
                    ws: {
                        origin: new origins.HttpOrigin(websocketApiDomainName),
                        cachePolicy: CachePolicy.CACHING_DISABLED,
                        originRequestPolicy:
                            OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                        responseHeadersPolicy:
                            ResponseHeadersPolicy.CORS_ALLOW_ALL_ORIGINS,
                        allowedMethods: AllowedMethods.ALLOW_ALL,
                        viewerProtocolPolicy:
                            ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
                    },
                    'ws/*': {
                        origin: new origins.HttpOrigin(websocketApiDomainName),
                        cachePolicy: CachePolicy.CACHING_DISABLED,
                        originRequestPolicy:
                            OriginRequestPolicy.ALL_VIEWER_EXCEPT_HOST_HEADER,
                        responseHeadersPolicy:
                            ResponseHeadersPolicy.CORS_ALLOW_ALL_ORIGINS,
                        allowedMethods: AllowedMethods.ALLOW_ALL,
                        viewerProtocolPolicy:
                            ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
                    },
                },
                enableLogging: true,
                httpVersion: HttpVersion.HTTP2_AND_3,
            },
        },
    });

    stack.addOutputs({
        ApiEndpoint: api.url,
        WebsocketApiEndpoint: websocketApi.url,
        host: `https://${customDomain.domainName}`,
        wsHost: `wss://${customDomain.domainName}/ws`,
    });
}
