use std::path::PathBuf;

fn main() {
    let proto = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../proto/p2pchat.proto");

    println!("cargo:rerun-if-changed={}", proto.display());

    // vendored protoc — не требует системной установки
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    std::env::set_var("PROTOC", protoc);

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    prost_build::Config::new()
        .out_dir(&out_dir)
        .compile_protos(
            &[proto],
            &[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../proto")],
        )
        .expect("prost build");
}
