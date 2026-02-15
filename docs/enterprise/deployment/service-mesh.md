---
title: Service Mesh
description: Integrating Cello with Istio, Envoy, and service mesh infrastructure
---

# Service Mesh

A service mesh provides infrastructure-level features like mutual TLS (mTLS), traffic management, and observability without changing application code. This guide covers integrating Cello with Istio and Envoy.

---

## What a Service Mesh Provides

| Feature | Description |
|---------|-------------|
| **mTLS** | Automatic encryption between all services |
| **Service Discovery** | Services find each other by name |
| **Traffic Management** | Canary deployments, A/B testing, circuit breaking |
| **Observability** | Distributed tracing, metrics, and access logs |
| **Retries & Timeouts** | Automatic retry policies at the mesh level |

---

## Istio Integration

### Installing Istio

```bash
istioctl install --set profile=demo
kubectl label namespace default istio-injection=enabled
```

When Istio injection is enabled, a sidecar proxy (Envoy) is automatically added to every pod in the namespace. No changes to your Cello application are needed.

### Deployment with Istio

Deploy Cello normally. The Istio sidecar intercepts all traffic:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cello-app
  labels:
    app: cello-app
    version: v1
spec:
  replicas: 3
  template:
    metadata:
      labels:
        app: cello-app
        version: v1
    spec:
      containers:
        - name: app
          image: cello-app:latest
          ports:
            - containerPort: 8000
```

---

## Mutual TLS (mTLS)

Istio provides automatic mTLS between services. Enable strict mTLS:

```yaml
apiVersion: security.istio.io/v1beta1
kind: PeerAuthentication
metadata:
  name: default
spec:
  mtls:
    mode: STRICT
```

With strict mTLS, all service-to-service communication is encrypted without any application-level TLS configuration.

---

## Traffic Management

### Virtual Service (Routing)

```yaml
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: cello-app
spec:
  hosts:
    - cello-app
  http:
    - route:
        - destination:
            host: cello-app
            subset: v1
          weight: 90
        - destination:
            host: cello-app
            subset: v2
          weight: 10
```

This sends 90% of traffic to v1 and 10% to v2 (canary deployment).

### Destination Rule

```yaml
apiVersion: networking.istio.io/v1beta1
kind: DestinationRule
metadata:
  name: cello-app
spec:
  host: cello-app
  trafficPolicy:
    connectionPool:
      tcp:
        maxConnections: 100
      http:
        h2UpgradePolicy: DEFAULT
        maxRequestsPerConnection: 100
    outlierDetection:
      consecutive5xxErrors: 5
      interval: 30s
      baseEjectionTime: 30s
  subsets:
    - name: v1
      labels:
        version: v1
    - name: v2
      labels:
        version: v2
```

---

## Circuit Breaking at the Mesh Level

The mesh provides circuit breaking independently of Cello's built-in circuit breaker:

```yaml
trafficPolicy:
  outlierDetection:
    consecutive5xxErrors: 5
    interval: 30s
    baseEjectionTime: 60s
    maxEjectionPercent: 50
```

!!! tip
    Use Cello's circuit breaker for application-level protection and the mesh circuit breaker for infrastructure-level protection. They complement each other.

---

## Observability

Istio automatically collects:

- **Metrics** via Envoy sidecar (request count, latency, error rate)
- **Distributed traces** via trace context propagation
- **Access logs** for every request

View the service graph in Kiali:

```bash
istioctl dashboard kiali
```

---

## Retry Policies

Configure automatic retries at the mesh level:

```yaml
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: cello-app
spec:
  hosts:
    - cello-app
  http:
    - retries:
        attempts: 3
        perTryTimeout: 2s
        retryOn: "5xx,reset,connect-failure"
      route:
        - destination:
            host: cello-app
```

---

## Timeouts

```yaml
http:
  - timeout: 10s
    route:
      - destination:
          host: cello-app
```

---

## Cello + Service Mesh Best Practices

| Concern | Recommendation |
|---------|---------------|
| **TLS** | Use mesh mTLS instead of Cello's `TlsConfig` for internal traffic |
| **Health Checks** | Keep Cello health endpoints; probes bypass the sidecar |
| **Metrics** | Use both Cello Prometheus and mesh metrics for full visibility |
| **Tracing** | Propagate `traceparent` headers through your handlers |
| **Circuit Breaking** | Use both layers for defense in depth |

---

## Next Steps

- See the [Kubernetes guide](kubernetes.md) for base deployment manifests.
- See [OpenTelemetry](../observability/opentelemetry.md) for application-level tracing.
- See the [Microservices tutorial](../../learn/tutorials/microservices.md) for building services.
