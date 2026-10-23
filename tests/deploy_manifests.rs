//! S-006: the committed Kubernetes manifests must carry a gRPC readiness probe
//! the API server accepts and that probes the classify service.
//!
//! Kubernetes decodes a `grpc:` probe's `port` as an integer: unlike HTTP and
//! TCP probes it cannot name a container port, and a named one fails the whole
//! Deployment at apply time. Naming `service` makes the probe report the
//! classify API's readiness rather than the empty-name whole-server status.
//!
//! A plain `#[test]` (no cluster) that reads every manifest under `deploy/` and
//! parses each `grpc:` probe from its flow (`{port: ..., service: ...}`) or block
//! mapping.

use llm_d_sc::grpc::classify::SERVICE_NAME;
use std::collections::HashMap;
use std::path::PathBuf;

/// One `grpc:` probe: the manifest it is in and its fields.
struct GrpcProbe {
    manifest: String,
    fields: HashMap<String, String>,
}

fn key_value(pair: &str) -> Option<(String, String)> {
    let (key, value) = pair.split_once(':')?;
    Some((key.trim().to_string(), value.trim().to_string()))
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn grpc_probes() -> Vec<GrpcProbe> {
    let deploy = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("deploy");
    let mut manifests: Vec<PathBuf> = std::fs::read_dir(&deploy)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", deploy.display()))
        .map(|entry| entry.expect("deploy/ entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    manifests.sort();

    let mut probes = Vec::new();
    for path in manifests {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let lines: Vec<&str> = source.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(rest) = line.trim_start().strip_prefix("grpc:") else {
                continue;
            };
            let fields = match rest.trim().strip_prefix('{') {
                Some(flow) => flow
                    .trim_end_matches('}')
                    .split(',')
                    .filter_map(key_value)
                    .collect(),
                None => lines[i + 1..]
                    .iter()
                    .take_while(|next| indent(next) > indent(line))
                    .filter_map(|next| key_value(next))
                    .collect(),
            };
            probes.push(GrpcProbe {
                manifest: path.file_name().unwrap().to_string_lossy().into_owned(),
                fields,
            });
        }
    }
    probes
}

#[test]
fn grpc_readiness_probes_use_a_numeric_port_and_the_classify_service() {
    let probes = grpc_probes();
    assert!(
        !probes.is_empty(),
        "deploy/ must keep at least one gRPC readiness probe"
    );

    for probe in probes {
        let port = probe.fields.get("port").map(String::as_str);
        assert!(
            port.is_some_and(|p| p.parse::<u16>().is_ok_and(|p| p > 0)),
            "{}: grpc probe port must be a port number, got {port:?}",
            probe.manifest
        );
        assert_eq!(
            probe.fields.get("service").map(String::as_str),
            Some(SERVICE_NAME),
            "{}: grpc probe must name the classify service",
            probe.manifest
        );
    }
}
