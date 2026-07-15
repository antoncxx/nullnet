use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tun_rs::AsyncDevice;

use crate::crypto;
use crate::forward::frame::Frame;
use crate::peers::peer::{Peers, VlanCipher};

/// Handles incoming network packets (receives packets from the socket and
/// writes them to the TAP interface).
pub async fn receive(
    device: &Arc<AsyncDevice>,
    socket: &Arc<UdpSocket>,
    peers: &Arc<RwLock<Peers>>,
) {
    let mut frame = Frame::new();
    loop {
        // wait until there is an incoming datagram on the socket
        let Ok((s, _)) = socket.recv_from(&mut frame.frame).await else {
            continue;
        };
        frame.size = s;

        if frame.size > 0 {
            let datagram = &frame.frame[..frame.size];
            // the vlan_id has to be readable before decryption so we know
            // which tunnel's key to decrypt with
            let Some((vlan_id, sealed)) = crypto::open_vlan_id(datagram) else {
                continue;
            };
            let Some(vlan_key) = peers.read().await.get_key(vlan_id) else {
                continue;
            };
            // decrypt as the packet exits the tunnel; auth failure (wrong
            // key, corrupted/spoofed datagram) drops it here. Plaintext
            // tunnels have nothing to authenticate — the bytes are the frame.
            let pkt_data = match vlan_key {
                VlanCipher::Encrypted(cipher) => {
                    let Some(data) = cipher.decrypt(sealed) else {
                        continue;
                    };
                    data
                }
                VlanCipher::Plaintext => sealed.to_vec(),
            };

            // write packet to the kernel
            device.send(&pkt_data).await.unwrap_or(0);
        }
    }
}
