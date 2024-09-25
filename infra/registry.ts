async function createRepo() {
    return await digitalocean.getContainerRegistry({ name: 'moosicbox' });
}
export const repo = await createRepo();

const creds = new digitalocean.ContainerRegistryDockerCredentials(
    'moosicbox-creds',
    {
        registryName: repo.name,
        write: true,
    },
);

export const registryAuth = creds.dockerCredentials.apply((authJson) => {
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
