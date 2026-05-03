use std::collections::HashMap;
use std::net::TcpListener;

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo};

use super::SERVICE_TYPE;

pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
    port: u16,
}

impl Advertiser {
    pub fn new(device_name: &str) -> Result<(Self, TcpListener)> {
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

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &format!("{hostname}.local."),
            "",
            port,
            properties,
        )?
        .enable_addr_auto();

        let fullname = service_info.get_fullname().to_string();
        daemon.register(service_info)?;

        tracing::info!("mDNS: advertising {instance_name} on port {port}");

        Ok((
            Self {
                daemon,
                fullname,
                port,
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

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &format!("{hostname}.local."),
            "",
            self.port,
            properties,
        )?
        .enable_addr_auto();

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
