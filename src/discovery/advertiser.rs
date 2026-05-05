use std::collections::HashMap;
use std::net::{Ipv4Addr, TcpListener};

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};

use super::SERVICE_TYPE;

fn get_lan_ipv4_addrs() -> Vec<Ipv4Addr> {
    let Ok(interfaces) = NetworkInterface::show() else {
        return vec![];
    };
    interfaces
        .into_iter()
        .filter(|iface| {
            let name = &iface.name;
            !name.starts_with("utun")
                && !name.starts_with("tun")
                && !name.starts_with("ppp")
                && !name.starts_with("lo")
                && !name.starts_with("bridge")
        })
        .flat_map(|iface| iface.addr)
        .filter_map(|addr| match addr {
            network_interface::Addr::V4(v4) => {
                let ip = v4.ip;
                if ip.is_loopback() || ip.is_link_local() {
                    None
                } else {
                    Some(ip)
                }
            }
            _ => None,
        })
        .collect()
}

fn build_service_info(
    instance_name: &str,
    hostname: &str,
    port: u16,
    properties: HashMap<String, String>,
) -> Result<ServiceInfo> {
    let lan_addrs = get_lan_ipv4_addrs();
    let ip_str = lan_addrs
        .iter()
        .map(|a| a.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let info = if ip_str.is_empty() {
        ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &format!("{hostname}.local."),
            "",
            port,
            properties,
        )?
        .enable_addr_auto()
    } else {
        ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &format!("{hostname}.local."),
            ip_str.as_str(),
            port,
            properties,
        )?
    };
    Ok(info)
}

pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
    port: u16,
    http_port: u16,
}

impl Advertiser {
    pub fn new(device_name: &str, http_port: u16) -> Result<(Self, TcpListener)> {
        let listener = TcpListener::bind("0.0.0.0:0")?;
        let port = listener.local_addr()?.port();

        let daemon = ServiceDaemon::new()?;

        let hostname = hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let instance_name = if device_name.is_empty() {
            &hostname
        } else {
            device_name
        };

        let mut properties = HashMap::new();
        properties.insert("device_name".to_string(), device_name.to_string());
        properties.insert("http_port".to_string(), http_port.to_string());

        let service_info = build_service_info(instance_name, &hostname, port, properties)?;
        let fullname = service_info.get_fullname().to_string();
        daemon.register(service_info)?;

        tracing::info!("mDNS: advertising {instance_name} on port {port}");

        Ok((
            Self {
                daemon,
                fullname,
                port,
                http_port,
            },
            listener,
        ))
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn unregister(&self) -> Result<()> {
        self.daemon.unregister(&self.fullname)?;
        tracing::info!("mDNS: unregistered service");
        Ok(())
    }

    pub fn reregister(&self, device_name: &str) -> Result<()> {
        let hostname = hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let instance_name = if device_name.is_empty() {
            &hostname
        } else {
            device_name
        };

        let mut properties = HashMap::new();
        properties.insert("device_name".to_string(), device_name.to_string());
        properties.insert("http_port".to_string(), self.http_port.to_string());

        let service_info = build_service_info(instance_name, &hostname, self.port, properties)?;
        self.daemon.register(service_info)?;
        tracing::info!("mDNS: re-registered service on port {}", self.port);
        Ok(())
    }
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}
