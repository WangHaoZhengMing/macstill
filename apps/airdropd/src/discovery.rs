//! Nearby peer discovery and link-local address scoping.
use super::*;
pub(super) async fn discover(
    addr: SocketAddrV6,
    id: String,
    flags: u32,
    events: mpsc::UnboundedSender<Internal>,
) {
    let Ok(sender) = Sender::new(sender::Config::new("Macstill", id.clone())) else {
        return;
    };
    if let Ok(Ok(Some(p))) =
        tokio::time::timeout(Duration::from_secs(3), sender.discover(addr)).await
    {
        let _ = events.send(Internal::Peer(
            Peer {
                id: addr.to_string(),
                name: p.computer_name,
                model: p.model_name.unwrap_or_default(),
            },
            addr,
            flags,
        ));
    }
}
pub(super) fn peer_address(mac: &str, index: u32) -> Option<SocketAddrV6> {
    let b = mac
        .split(':')
        .map(|b| u8::from_str_radix(b, 16))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if b.len() != 6 {
        return None;
    }
    Some(SocketAddrV6::new(
        Ipv6Addr::new(
            0xfe80,
            0,
            0,
            0,
            u16::from_be_bytes([b[0] ^ 2, b[1]]),
            u16::from_be_bytes([b[2], 0xff]),
            u16::from_be_bytes([0xfe, b[3]]),
            u16::from_be_bytes([b[4], b[5]]),
        ),
        8770,
        0,
        index,
    ))
}
