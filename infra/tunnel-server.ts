import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { certManagers } from './cert-manager';
import { Input, Resource } from '@pulumi/pulumi';
import { loadBalancer } from './load-balancer';

function createEcrRepo() {
    return new awsx.ecr.Repository('moosicbox-tunnel-server', {
        forceDelete: true,
    });
}

function createImage(repo: awsx.ecr.Repository) {
    const context = `../..`;
    return new awsx.ecr.Image('tunnel-server', {
        repositoryUrl: repo.url,
        context,
        dockerfile: `${context}/packages/tunnel_server/TunnelServer.Dockerfile`,
    });
}

function createDeployment(
    image: awsx.ecr.Image,
    dependsOn: Input<Input<Resource>[]>,
) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-deployment.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-server',
        ` moosicbox-tunnel-server-${$app.stage}`,
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-service',
        ` moosicbox-tunnel-service-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    const containers = specJson.spec.template.spec.containers;

    containers.forEach(
        (container: {
            env: Record<string, string>[] | undefined;
            image: string | awsx.ecr.Image['imageUri'];
        }) => {
            container.image = image.imageUri;
        },
    );

    return new kubernetes.apps.v1.Deployment('tunnel-server', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

function createService(dependsOn: Input<Input<Resource>[]>) {
    let specYaml = fs.readFileSync(
        '../kubernetes/tunnel-server/moosicbox-tunnel-server-service.yaml'.substring(
            1,
        ),
        'utf8',
    );
    specYaml = specYaml.replaceAll(
        ' moosicbox-tunnel-service',
        ` moosicbox-tunnel-service-${$app.stage}`,
    );
    const specJson = parse(specYaml);

    return new kubernetes.core.v1.Service('tunnel-server', specJson, {
        provider: clusterProvider,
        dependsOn,
    });
}

const repo = createEcrRepo();
const image = createImage(repo);
const tunnelServerDeployment = createDeployment(image, [
    image,
    certManagers,
    loadBalancer,
]);
const tunnelServerService = createService([tunnelServerDeployment]);

export const outputs = {
    serviceId: tunnelServerService.id,
    availableReplicas: tunnelServerDeployment.status.availableReplicas,
};
