use crate::config::Config;

#[must_use]
pub fn guest_network_xml(config: &Config) -> String {
    format!(
        "<network>\n  <name>{}</name>\n  <bridge name='{}' stp='on' delay='0'/>\n</network>\n",
        xml_text(&config.network.guest_network),
        xml_attribute(&config.network.guest_bridge),
    )
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_attribute(value: &str) -> String {
    xml_text(value)
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_network_has_no_host_ip_dhcp_or_forwarding() {
        let xml = guest_network_xml(&crate::config::tests::valid());
        assert!(xml.contains("<name>tailnet-android-guest</name>"));
        assert!(xml.contains("bridge name='vmbr-android'"));
        assert!(!xml.contains("<ip"));
        assert!(!xml.contains("<forward"));
        assert!(!xml.contains("<dhcp"));
    }
}
