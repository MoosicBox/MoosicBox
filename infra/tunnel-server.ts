import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { Input, Resource } from '@pulumi/pulumi';

function createEcrRepo() {
    return new awsx.ecr.Repository('moosicbox-tunnel-server', {
        forceDelete: true,
    });
}

function createImage(repo: awsx.ecr.Repository) {
    const context = `../..`;
    const authToken = aws.ecr.getAuthorizationTokenOutput({
        registryId: repo.repository.registryId,
    });
    return new docker.Image('tunnel-server', {
        imageName: 'tunnel-server',
        build: {
            platform: 'linux/amd64',
            context: context,
            dockerfile: `${context}/packages/tunnel_server/TunnelServer.Dockerfile`,
        },
        registry: {
            server: repo.repository.repositoryUrl,
            password: authToken.password,
            username: authToken.userName,
        },
    });
}

function createDeployment(
    image: docker.Image,
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
            container.image = $interpolate`${image.imageName}`;
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
const tunnelServerDeployment = createDeployment(image, []);
// const tunnelServerService = createService([]);

export const outputs = {
    // serviceId: tunnelServerService.id,
    availableReplicas: tunnelServerDeployment.status.availableReplicas,
};
