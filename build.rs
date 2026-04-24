fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Health service (placeholder até REAPI protos serem vendados)
    tonic_build::compile_protos("proto/health.proto")?;

    // TODO semana 1: vendor REAPI protos oficiais de bazelbuild/remote-apis
    // Deve incluir: remote_execution.proto + deps (google/bytestream, google/rpc, build/bazel/semver)
    // tonic_build::configure()
    //     .build_server(true)
    //     .build_client(false)
    //     .compile_protos(
    //         &["proto/build/bazel/remote/execution/v2/remote_execution.proto"],
    //         &["proto"],
    //     )?;

    Ok(())
}
