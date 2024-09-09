import { parse } from 'yaml';
import fs from 'node:fs';
import { clusterProvider } from './cluster';
import { certManager } from './cert-manager';

function createCertificate() {
    let tunnelServerCertificateSpecYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-certificate.yaml'.substring(
            1,
        ),
        'utf8',
    );
    tunnelServerCertificateSpecYaml =
        tunnelServerCertificateSpecYaml.replaceAll(
            ' moosicbox-tunnel-server-cert',
            ` moosicbox-tunnel-server-cert-${$app.stage}`,
        );
    tunnelServerCertificateSpecYaml =
        tunnelServerCertificateSpecYaml.replaceAll(
            ' moosicbox-tunnel-server-issuer',
            ` moosicbox-tunnel-server-issuer-${$app.stage}`,
        );
    tunnelServerCertificateSpecYaml =
        tunnelServerCertificateSpecYaml.replaceAll(
            ' tunnel.moosicbox.com',
            ` tunnel-${$app.stage}.moosicbox.com`,
        );

    return kubernetes.yaml.parse(
        { yaml: tunnelServerCertificateSpecYaml },
        { provider: clusterProvider, dependsOn: [certManager] },
    )[0];
}

function createIngress() {
    let tunnelServerIngressSpecYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-ingress.yaml'.substring(
            1,
        ),
        'utf8',
    );
    tunnelServerIngressSpecYaml = tunnelServerIngressSpecYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    const tunnelServerIngressSpec = parse(tunnelServerIngressSpecYaml);

    return new kubernetes.networking.v1.Ingress(
        'tunnel-server',
        tunnelServerIngressSpec,
        { provider: clusterProvider, dependsOn: [certManager] },
    );
}

function createIssuer() {
    let tunnelServerIssuerSpecYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-issuer.yaml'.substring(
            1,
        ),
        'utf8',
    );
    tunnelServerIssuerSpecYaml = tunnelServerIssuerSpecYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    tunnelServerIssuerSpecYaml = tunnelServerIssuerSpecYaml.replaceAll(
        ' moosicbox-tunnel-server-issuer',
        ` moosicbox-tunnel-server-issuer-${$app.stage}`,
    );

    return kubernetes.yaml.parse(
        { yaml: tunnelServerIssuerSpecYaml },
        { provider: clusterProvider, dependsOn: [certManager] },
    )[0];
}

function createLb() {
    let tunnelServerLbDeploymentSpecYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-lb-deployment.yaml'.substring(
            1,
        ),
        'utf8',
    );
    tunnelServerLbDeploymentSpecYaml =
        tunnelServerLbDeploymentSpecYaml.replaceAll(
            ' moosicbox-tunnel-server-lb',
            ` moosicbox-tunnel-server-lb-${$app.stage}`,
        );
    tunnelServerLbDeploymentSpecYaml =
        tunnelServerLbDeploymentSpecYaml.replaceAll(
            ' moosicbox-tunnel-server-cert',
            ` moosicbox-tunnel-server-cert-${$app.stage}`,
        );
    const tunnelServerLbDeploymentSpec = parse(
        tunnelServerLbDeploymentSpecYaml,
    );

    return new kubernetes.apps.v1.Deployment(
        'tunnel-server-lb',
        tunnelServerLbDeploymentSpec,
        { provider: clusterProvider, dependsOn: [certManager] },
    );
}

function createNodePort() {
    let tunnelServerNodePortSpecYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-nodeport.yaml'.substring(
            1,
        ),
        'utf8',
    );
    tunnelServerNodePortSpecYaml = tunnelServerNodePortSpecYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress-controller',
        ` moosicbox-tunnel-server-ingress-controller-${$app.stage}`,
    );
    tunnelServerNodePortSpecYaml = tunnelServerNodePortSpecYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    tunnelServerNodePortSpecYaml = tunnelServerNodePortSpecYaml.replaceAll(
        ' tunnel.moosicbox.com',
        ` tunnel-${$app.stage}.moosicbox.com`,
    );

    return kubernetes.yaml.parse(
        { yaml: tunnelServerNodePortSpecYaml },
        { provider: clusterProvider, dependsOn: [certManager] },
    )[0];
}

createCertificate();
createIngress();
createIssuer();
createLb();
createNodePort();

export const outputs = {};
