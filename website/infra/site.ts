function getCustomDomain() {
    return {
        name: domainName,
        dns: sst.cloudflare.dns(),
    };
}

const defaultStageName = 'prod';
const isDefaultStage = $app.stage === defaultStageName;
const domain = process.env.DOMAIN ?? 'moosicbox.com';
const slug = 'app';
const subdomain = isDefaultStage ? slug : `${slug}-${$app.stage}`;
const domainName = `${subdomain}.${domain}`;

const customDomain = getCustomDomain();

const site = new sst.aws.Astro('MoosicBox', {
    buildCommand: 'pnpm build --config astro.config.sst.mjs',
    domain: customDomain,
});

export const outputs = {
    url: site.url,
    host: `https://${domainName}`,
};
