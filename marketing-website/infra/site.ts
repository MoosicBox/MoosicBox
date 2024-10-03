function getCustomDomain() {
    return {
        name: domainName,
        dns: sst.cloudflare.dns(),
    };
}

const domain = process.env.DOMAIN || 'moosicbox.com';

if (!domain) throw new Error('Missing DOMAIN environment variable');

const isProd = $app.stage === 'prod';
const subdomain = isProd ? '' : `marketing-${$app.stage}.`;
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
