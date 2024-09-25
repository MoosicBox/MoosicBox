import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { Input, Output, Resource } from '@pulumi/pulumi';
import { registryAuth, repo } from './registry';

export const domainName = 'moosicbox.com';
export const domain =
    $app.stage === 'prod'
        ? `tunnel.${domainName}`
        : `tunnel-${$app.stage}.${domainName}`;

export const imageName = $interpolate`${repo.endpoint}/load-balancer`;

function createImage() {
    const context = `../..`;
    return new docker.Image('load-balancer', {
        imageName,
        build: {
            builderVersion: 'BuilderBuildKit',
            args: {
                BUILDKIT_INLINE_CACHE: '1',
            },
            platform: 'linux/amd64',
            context: context,
            dockerfile: `${context}/packages/load_balancer/LoadBalancer.Dockerfile`,
        },
        registry: registryAuth,
    });
}

function createCertificate(dependsOn: Input<Input<Resource>[]>) {
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
        { provider: clusterProvider, dependsOn },
    )[0];
}

function createIngress(dependsOn: Input<Input<Resource>[]>) {
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

    delete specJson.metadata.annotations['cert-manager.io/issuer'];

    return new kubernetes.networking.v1.Ingress('tunnel-server', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

function createIssuer(dependsOn: Input<Input<Resource>[]>) {
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
        { provider: clusterProvider, dependsOn },
    )[0];
}

function createLb(image: docker.Image, dependsOn: Input<Input<Resource>[]>) {
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

    type Container = {
        ports: { hostPort: number }[];
        volumeMounts: unknown[] | undefined;
        env: Record<string, string>[] | undefined;
        image: string | Output<string>;
    };

    specJson.spec.template.spec.volumes = [];

    const containers: Container[] = specJson.spec.template.spec.containers;

    containers.forEach((container) => {
        container.image = $interpolate`${image.imageName}`;
        container.volumeMounts = [];

        container.env = container.env ?? [];
        container.env.push({
            name: 'CLUSTERS',
            value: `${domain}:moosicbox-tunnel-service-${$app.stage}:8004`,
        });

        container.ports = container.ports.filter((x) => x.hostPort === 80);
    });

    return new kubernetes.apps.v1.Deployment('tunnel-server-lb', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

function createNodePort(dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-nodeport.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress-controller',
        ` moosicbox-tunnel-server-temp-ingress-controller-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-ingress',
        ` moosicbox-tunnel-server-ingress-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server-temp-',
        ` moosicbox-tunnel-server-`,
    );
    specYaml = specYaml.replaceAll(' tunnel.moosicbox.com', ` ${domain}`);

    return kubernetes.yaml.parse(
        { yaml: specYaml },
        { provider: clusterProvider, dependsOn },
    )[0];
}

export const image = createImage();
export const loadBalancer = createLb(image, []);
// export const certificate = createCertificate([]);
// export const issuer = createIssuer([]);
export const ingress = createIngress([loadBalancer]);
export const nodePort = createNodePort([loadBalancer]);

export const outputs = {};
