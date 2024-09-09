import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { certManagers } from './cert-manager';

export const domainName = 'moosicbox.com';
export const domain =
    $app.stage === 'prod'
        ? `tunnel.${domainName}`
        : `tunnel-${$app.stage}.${domainName}`;

function createCertificate() {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-certificate.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-cert',
        ` moosicbox-tunnel-server-cert-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-issuer',
        ` moosicbox-tunnel-server-issuer-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(' tunnel.moosicbox.com', ` ${domain}`);

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn: [certManagers] },
    )[0];
}

function createIngress() {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-ingress.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-issuer',
        ` moosicbox-tunnel-server-issuer-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    return new kubernetes.networking.v1.Ingress('tunnel-server', specJson, {
        provider: clusterProvider,
        dependsOn: [certManagers],
    });
}

function createIssuer() {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-issuer.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-issuer',
        ` moosicbox-tunnel-server-issuer-${$app.stage}`,
    );

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn: [certManagers] },
    )[0];
}

function createLb() {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-lb-deployment.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-lb',
        ` moosicbox-tunnel-server-lb-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-cert',
        ` moosicbox-tunnel-server-cert-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    const containers = specJson.spec.template.spec.containers;

    containers.forEach(
        (container: { env: Record<string, string>[] | undefined }) => {
            const env = container.env ?? [];

            env.push({
                name: 'CLUSTERS',
                value: `${domain}:moosicbox-tunnel-service-${$app.stage}:8004`,
            });

            container.env = env;
        },
    );

    return new kubernetes.apps.v1.Deployment('tunnel-server-lb', specJson, {
        provider: clusterProvider,
        dependsOn: [certManagers],
    });
}

function createNodePort() {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-nodeport.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress-controller',
        ` moosicbox-tunnel-server-ingress-controller-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(' tunnel.moosicbox.com', ` ${domain}`);

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn: [certManagers] },
    )[0];
}

createCertificate();
createIngress();
createIssuer();
createLb();
createNodePort();

export const outputs = {};
