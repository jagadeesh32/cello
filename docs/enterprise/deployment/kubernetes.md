---
title: Kubernetes Deployment
description: Deploying Cello applications on Kubernetes with Deployments, Services, and autoscaling
---

# Kubernetes Deployment

This guide provides Kubernetes manifests for deploying Cello applications, including Deployments, Services, Ingress, HPA, ConfigMaps, and Secrets.

> **Note on multi-worker mode:** Kubernetes pods run Linux containers, so Cello's multi-worker mode uses the `os.fork()` + `SO_REUSEPORT` strategy for best performance. The `CELLO_WORKER` environment variable is reserved for internal use by Cello's worker management on Windows and should not be set in ConfigMaps or pod specs.

---

## Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cello-app
  labels:
    app: cello-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: cello-app
  template:
    metadata:
      labels:
        app: cello-app
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8000"
        prometheus.io/path: "/metrics"
    spec:
      containers:
        - name: app
          image: registry.example.com/cello-app:latest
          ports:
            - containerPort: 8000
          envFrom:
            - configMapRef:
                name: cello-config
            - secretRef:
                name: cello-secrets
          resources:
            requests:
              cpu: "250m"
              memory: "128Mi"
            limits:
              cpu: "1000m"
              memory: "512Mi"
          livenessProbe:
            httpGet:
              path: /health/live
              port: 8000
            initialDelaySeconds: 5
            periodSeconds: 10
            failureThreshold: 3
          readinessProbe:
            httpGet:
              path: /health/ready
              port: 8000
            initialDelaySeconds: 5
            periodSeconds: 5
            failureThreshold: 3
          startupProbe:
            httpGet:
              path: /health/startup
              port: 8000
            initialDelaySeconds: 0
            periodSeconds: 2
            failureThreshold: 30
```

---

## Service

```yaml
apiVersion: v1
kind: Service
metadata:
  name: cello-app
spec:
  selector:
    app: cello-app
  ports:
    - port: 80
      targetPort: 8000
      protocol: TCP
  type: ClusterIP
```

---

## Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: cello-ingress
  annotations:
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
spec:
  tls:
    - hosts:
        - api.example.com
      secretName: cello-tls
  rules:
    - host: api.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: cello-app
                port:
                  number: 80
```

---

## Horizontal Pod Autoscaler (HPA)

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: cello-app-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: cello-app
  minReplicas: 2
  maxReplicas: 20
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
    - type: Resource
      resource:
        name: memory
        target:
          type: Utilization
          averageUtilization: 80
```

---

## ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: cello-config
data:
  CELLO_ENV: production
  WORKERS: "4"
  HOST: "0.0.0.0"
  PORT: "8000"
```

---

## Secrets

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: cello-secrets
type: Opaque
stringData:
  JWT_SECRET: "your-production-secret-here"
  DATABASE_URL: "postgresql://user:pass@db-host:5432/app"
```

!!! warning
    In production, use a secrets manager (Vault, AWS Secrets Manager) instead of plaintext Kubernetes Secrets.

---

## Namespace

Isolate the application in its own namespace:

```bash
kubectl create namespace cello-production
kubectl apply -f k8s/ -n cello-production
```

---

## Rolling Updates

The Deployment spec supports rolling updates by default. Customize the strategy:

```yaml
spec:
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
```

This ensures zero-downtime deployments by keeping all existing pods running until new pods pass health checks.

---

## Deploying

```bash
# Apply all manifests
kubectl apply -f k8s/

# Check status
kubectl get pods -l app=cello-app
kubectl get svc cello-app
kubectl get ingress cello-ingress

# View logs
kubectl logs -l app=cello-app -f

# Scale manually
kubectl scale deployment cello-app --replicas=5
```

---

## Next Steps

- See [Health Checks](../observability/health-checks.md) for probe configuration.
- See [Service Mesh](service-mesh.md) for mTLS and traffic management.
- See [Metrics](../observability/metrics.md) for Prometheus monitoring with HPA.
