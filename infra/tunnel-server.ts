import fs from 'node:fs';
import { parse } from 'yaml';
import { clusterProvider } from './cluster';
import { Input, Output, Resource } from '@pulumi/pulumi';

async function createRepo() {
    return await digitalocean.getContainerRegistry({ name: 'moosicbox' });
}
const repo = await createRepo();

const imageName = $interpolate`${repo.endpoint}/tunnel-server`;
const creds = new digitalocean.ContainerRegistryDockerCredentials(
    'moosicbox-creds',
    {
        registryName: repo.name,
        write: true,
    },
);

const registryInfo = creds.dockerCredentials.apply((authJson) => {
    // We are given a Docker creds file; parse it to find the temp username/password.
    const auths = JSON.parse(authJson);
    const authToken = auths['auths'][repo.serverUrl]['auth'];
    const decoded = Buffer.from(authToken, 'base64').toString();
    const [username, password] = decoded.split(':');
    if (!password || !username) {
        throw new Error('Invalid credentials');
    }
    return {
        server: repo.serverUrl,
        username: username,
        password: password,
    };
});

function createImage() {
    const context = `../..`;
    return new docker.Image('tunnel-server', {
        imageName,
        build: {
            platform: 'linux/amd64',
            context: context,
            dockerfile: `${context}/packages/tunnel_server/TunnelServer.Dockerfile`,
        },
        registry: registryInfo,
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
            env: { [key: string]: string | undefined } | undefined;
            image:
            | string
            | awsx.ecr.Image['imageUri']
            | {
                username: Output<string>;
                password: Output<string>;
                name: Output<string>;
            };
        }) => {
            container.image = $interpolate`${image.imageName}`;

            container.env = container.env ?? {};
            container.env['AWS_ACCESS_KEY_ID'] = process.env.AWS_ACCESS_KEY_ID;
            container.env['AWS_SECRET_ACCESS_KEY'] =
                process.env.AWS_SECRET_ACCESS_KEY;
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

const image = createImage();
const tunnelServerDeployment = createDeployment(image, []);
const tunnelServerService = createService([]);

export const outputs = {
    serviceId: tunnelServerService.id,
    availableReplicas: tunnelServerDeployment.status.availableReplicas,
};
