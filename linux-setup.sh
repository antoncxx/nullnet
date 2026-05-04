#!/bin/bash

sudo apt-get update && \
sudo apt-get install -y iptables conntrack openvswitch-switch && \
git pull && \
cargo xtask build --release && \
sudo cp nullnet-client.service /etc/systemd/system/ && \
sudo systemctl enable nullnet-client && \
sudo systemctl restart nullnet-client
