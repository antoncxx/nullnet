#!/bin/bash

sudo apt-get update && \
sudo apt-get install -y unzip && \
curl -OL https://github.com/google/protobuf/releases/download/v3.20.3/protoc-3.20.3-linux-x86_64.zip && \
unzip -o protoc-3.20.3-linux-x86_64.zip -d protoc3 && \
sudo mv protoc3/bin/* /usr/local/bin/ && \
sudo mv protoc3/include/* /usr/local/include/ && \
git pull && \
cargo b --release && \
sudo cp nullnet-proxy.service /etc/systemd/system/ && \
sudo systemctl enable nullnet-proxy && \
sudo systemctl restart nullnet-proxy
