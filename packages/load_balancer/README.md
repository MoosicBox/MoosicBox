# Load Balancer

## Certificate Setup

1. Add 'solver' to CLUSTERS env var. e.g. `solver:10.244.0.68:8089;...`
    1. Grab the IP from the solver pod via `kubectl describe pods cm-acme-http-solver-xxxxx`
2. Execute the solver url with the required token e.g. `https://tunnel.moosicbox.com/.well-known/acme-challenge/{token}`
    - (this should happen automatically)
3. Profit

### Tear down cluster

```
kubectl delete all --all

kubectl get validatingwebhookconfiguration.admissionregistration.k8s.io
kubectl delete validatingwebhookconfiguration.admissionregistration.k8s.io/<Junk Object>

kubectl get mutatingwebhookconfiguration.admissionregistration.k8s.io
kubectl delete mutatingwebhookconfiguration.admissionregistration.k8s.io/<Junk Object>
```

May need to manually `kubectl apply -f kubernetes/cert-manager.yaml`?
