set -e
kubectl config use-context kind-kind
helm uninstall malstrom-k8s-operator || true

CHART_PATH=./operator/helm/malstrom-k8s-operator
helm install malstrom-k8s-operator \
-f $CHART_PATH/local-values.yaml $CHART_PATH