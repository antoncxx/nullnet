const NULLNET_GRPC_PATH: &str = "./proto/nullnet_grpc.proto";
const PROTOBUF_DIR_PATH: &str = "./proto";

fn main() {
    tonic_prost_build::configure()
        .out_dir("./src/proto")
        .compile_protos(&[NULLNET_GRPC_PATH], &[PROTOBUF_DIR_PATH])
        .expect("Protobuf files generation failed");
}
