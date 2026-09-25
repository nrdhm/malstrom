//! Generates the MalstromJob CRD YAML for the helm chart.
//! (The gRPC stubs used by this crate come from `malstrom-k8s-proto`; they are no
//! longer compiled here.)
use std::fs::File;

use crds::{CustomResourceExt, MalstromJob};

fn main() {
    let dir = std::path::Path::new("../helm/malstrom-k8s-operator/crds");
    if dir.exists() {
        let writer = File::create(dir.join("MalstromJob.yaml")).expect("create CRD yaml");
        serde_yaml::to_writer(writer, &MalstromJob::crd()).expect("write CRD yaml");
    }

    println!("cargo:rerun-if-changed=src");
}
