function getCustomDomain() {
    return {
        name: domainName,
        dns: sst.cloudflare.dns(),
    };
}

const isProd = $app.stage === 'prod';
const subdomain = isProd ? '' : `marketing-${$app.stage}.`;
const domain = process.env.DOMAIN ?? 'moosicbox.com';
const domainName = `${subdomain}${domain}`;

const customDomain = getCustomDomain();

const site = new sst.aws.Astro('MoosicBoxMarketing', {
    buildCommand: 'pnpm build --config astro.config.sst.mjs',
    domain: customDomain,
    environment: {},
});

export const outputs = {
    url: site.url,
    host: `https://${domainName}`,
};
