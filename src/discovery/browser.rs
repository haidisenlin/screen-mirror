use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent};

use super::SERVICE_TYPE;

#[derive(Debug, Clone)]
pub struct DiscoveredReceiver {
    pub name: String,
    pub addr: SocketAddr,
}

pub fn browse(timeout: Duration) -> Result<Vec<DiscoveredReceiver>> {
    let daemon = ServiceDaemon::new()?;
    let receiver = daemon.browse(SERVICE_TYPE)?;

    let mut results = Vec::new();
    let deadline = std::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let port = info.get_port();
                let name = info
                    .get_property_val_str("device_name")
                    .unwrap_or_default()
                    .to_string();
                let display_name = if name.is_empty() {
                    info.get_fullname().to_string()
                } else {
                    name
                };

                for addr in info.get_addresses_v4() {
                    results.push(DiscoveredReceiver {
                        name: display_name.clone(),
                        addr: SocketAddr::new(IpAddr::V4(*addr), port),
                    });
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    daemon.shutdown()?;
    Ok(results)
}
