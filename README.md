# nullnet

Nullnet is a gRPC-based control plane that lets clients on different hosts expose services
to each other on-demand, building the required network infrastructure (VLAN / VXLAN) only
when a service is actually requested and tearing it down when it is no longer needed.

This repository is a Cargo workspace holding the three binaries that make up the architecture
plus the shared gRPC interface.

## Layout

```
.
├── members/
│   ├── nullnet-client/      # runs on each host, exposes local services to the control plane
│   ├── nullnet-server/      # control plane: orchestrates VLAN/VXLAN setup and tears them down
│   ├── nullnet-proxy/       # ingress proxy: maps `service_name:80` requests to the right host
│   └── nullnet-grpc-lib/    # shared gRPC interface + generated types (proto + build.rs live here)
├── ebpf/                    # eBPF program loaded by nullnet-client (nightly toolchain)
└── xtask/                   # builds the eBPF program + the client userspace
```

## Prerequisites

- install Rust
  ```
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- install protobuf (needed by `nullnet-grpc-lib`'s `build.rs`)
  ```
  curl -OL https://github.com/google/protobuf/releases/download/v3.20.3/protoc-3.20.3-linux-x86_64.zip
  unzip protoc-3.20.3-linux-x86_64.zip -d protoc3
  sudo mv protoc3/bin/* /usr/local/bin/
  sudo mv protoc3/include/* /usr/local/include/
  ```

The repository should be cloned under `/root/nullnet` so the provided `setup-*.sh` scripts and
`.service` units work without changes.

## Usage

### nullnet-server

- set environment variables (in `members/nullnet-server/.env`)
  ```
  NET_TYPE=VXLAN
  TIMEOUT=0
  ```

- service configuration must be stored at `members/nullnet-server/services/services.toml` and
  declare services as follows:
  ```
  [[services]]
  name = "color.com"
  timeout = 0
  proxy_dependencies = ["fs.color.com"]

  [[services.triggers]]
  port = 5555
  chain = ["ts.color.com"]

  [[services]]
  name = "fs.color.com"
  ```

- `proxy_dependencies` is a linear dep chain walked when the service is reached via a `Proxy`
  RPC from nullnet-proxy
- each `[[services.triggers]]` block pairs a port observed on the initiator's host with a linear
  chain walked when the service is reached via a `BackendTrigger` RPC from nullnet-client (one
  chain per port)

- run the project as a daemon (from the repo root)
  ```
  ./setup-server.sh
  ```

- the server will regularly update a view of the network and store it in `members/nullnet-server/graph.dot`

***

### nullnet-proxy

- set environment variables (in `members/nullnet-proxy/.env`; set `CONTROL_SERVICE_ADDR` to the IP
  of `nullnet-server`)
  ```
  CONTROL_SERVICE_ADDR=192.168.1.100
  CONTROL_SERVICE_PORT=50051
  ```

- run the project as a daemon (from the repo root)
  ```
  ./setup-proxy.sh
  ```

- the proxy will run on port 80 and receive requests in the form `service_name:80`

***

### nullnet-client

- set environment variables (in `members/nullnet-client/.env`; set `CONTROL_SERVICE_ADDR` to the IP
  of `nullnet-server`, `ETH_NAME` to the ethernet interface to monitor)
  ```
  CONTROL_SERVICE_ADDR=192.168.1.100
  CONTROL_SERVICE_PORT=50051
  ETH_NAME=ens18
  ```

- service configuration must be stored at `members/nullnet-client/services.toml`:
  ```
  # services = [] # use this if you don't want to declare any service

  [[services]]
  name = "color.com"
  port = 3001
  docker_container = "stack-name_container-name" # should correspond to the label "com.docker.swarm.service.name"

  [[services]]
  ...
  ```

- run the project as a daemon (from the repo root)
  ```
  ./setup-client.sh
  ```
