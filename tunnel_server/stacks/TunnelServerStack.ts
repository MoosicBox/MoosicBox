import { StackContext, Api, WebSocketApi, Stack } from 'sst/constructs';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
import {
    Distribution,
    DistributionDomainProps,
    // eslint-disable-next-line import/extensions
} from 'sst/constructs/Distribution.js';
import {
    AllowedMethods,
    CachePolicy,
    HttpVersion,
    OriginRequestPolicy,
    ResponseHeadersPolicy,
    ViewerProtocolPolicy,
} from 'aws-cdk-lib/aws-cloudfront';

const domainSlug = 'tunnel';
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

export async function API({ stack }: StackContext) {
    const customDomain = getCustomDomain(stack);

    const api = new Api(stack, 'api', {
        defaults: {
            function: {
                runtime: 'rust',
                timeout: '5 minutes',
                retryAttempts: 0,
                environment: {
                    RUST_LOG: process.env.RUST_LOG || 'info',
                    WS_HOST: `wss://${customDomain.domainName}/ws`,
                },
            },
        },
        routes: {
            'GET /track': 'src/moosicbox_tunnel_server.handler',
        },
    });

    const websocketApi = new WebSocketApi(stack, 'websockets', {
        defaults: {
            function: {
                runtime: 'rust',
                timeout: '5 minutes',
                retryAttempts: 0,
                environment: {
                    RUST_LOG: process.env.RUST_LOG || 'info',
                },
            },
        },
        routes: {
            $connect: 'src/moosicbox_tunnel_server_ws.handler',
            $default: 'src/moosicbox_tunnel_server_ws.handler',
            $disconnect: 'src/moosicbox_tunnel_server_ws.handler',
            sendMessage: 'src/moosicbox_tunnel_server_ws.handler',
        },
        cdk: {
            webSocketStage: {
                stageName: 'ws',
            },
        },
    });

    const apiDomainName = `${api.cdk.httpApi.httpApiId}.execute-api.${stack.region}.amazonaws.com`;
    const websocketApiDomainName = `${websocketApi.cdk.webSocketApi.apiId}.execute-api.${stack.region}.amazonaws.com`;

    // eslint-disable-next-line no-new
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
