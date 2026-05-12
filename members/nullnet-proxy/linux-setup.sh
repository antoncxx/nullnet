#!/bin/bash

git pull && \
cargo b --release && \
sudo cp nullnet-proxy.service /etc/systemd/system/ && \
sudo systemctl enable nullnet-proxy && \
sudo systemctl restart nullnet-proxy
