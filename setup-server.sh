#!/bin/bash

sudo apt-get update && \
sudo apt-get install -y unzip && \
{ command -v protoc >/dev/null || { \
  curl -OL https://github.com/google/protobuf/releases/download/v3.20.3/protoc-3.20.3-linux-x86_64.zip && \
  unzip -o protoc-3.20.3-linux-x86_64.zip -d protoc3 && \
  sudo mv protoc3/bin/* /usr/local/bin/ && \
  sudo mv protoc3/include/* /usr/local/include/ ; }; } && \
git pull && \
cargo build -p nullnet-server --release && \
sudo cp members/nullnet-server/nullnet-server.service /etc/systemd/system/ && \
sudo systemctl enable nullnet-server && \
sudo systemctl restart nullnet-server
