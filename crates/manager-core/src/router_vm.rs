use crate::config::Config;
use std::path::PathBuf;

#[must_use]
pub fn disk_path(config: &Config) -> PathBuf {
    config.storage.vm_dir.join("tailnet-router.qcow2")
}

#[must_use]
pub fn seed_path(config: &Config) -> PathBuf {
    config.storage.vm_dir.join("tailnet-router-seed.img")
}

#[must_use]
pub fn domain_xml(config: &Config) -> String {
    let disk = disk_path(config);
    let disk = xml_attribute(&disk.display().to_string());
    let seed = xml_attribute(&seed_path(config).display().to_string());
    let uplink = xml_attribute(&config.network.router_uplink_network);
    let guest = xml_attribute(&config.network.guest_network);
    format!(
        r#"<domain type='kvm'>
  <name>tailnet-android-router</name>
  <memory unit='MiB'>512</memory>
  <currentMemory unit='MiB'>512</currentMemory>
  <vcpu placement='static'>1</vcpu>
  <os>
    <type arch='x86_64' machine='q35'>hvm</type>
    <boot dev='hd'/>
  </os>
  <features><acpi/><apic/></features>
  <cpu mode='host-passthrough' check='none'/>
  <devices>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2' cache='none' discard='unmap'/>
      <source file='{disk}'/>
      <target dev='vda' bus='virtio'/>
    </disk>
    <disk type='file' device='cdrom'>
      <driver name='qemu' type='raw'/>
      <source file='{seed}'/>
      <target dev='sda' bus='sata'/>
      <readonly/>
    </disk>
    <interface type='network'>
      <mac address='52:54:00:00:00:01'/>
      <source network='{uplink}'/>
      <model type='virtio'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x03' function='0x0'/>
    </interface>
    <interface type='network'>
      <mac address='52:54:00:00:00:02'/>
      <source network='{guest}'/>
      <model type='virtio'/>
      <address type='pci' domain='0x0000' bus='0x00' slot='0x04' function='0x0'/>
    </interface>
    <serial type='pty'><target type='isa-serial' port='0'/></serial>
    <console type='pty'><target type='serial' port='0'/></console>
  </devices>
</domain>
"#
    )
}

fn xml_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn router_has_separate_uplink_and_guest_nics_and_no_secret() {
        let c = crate::config::tests::valid();
        let xml = domain_xml(&c);
        assert_eq!(xml.matches("<interface").count(), 2);
        assert!(xml.contains("tailnet-router-seed.img"));
        assert!(xml.contains("network='default'"));
        assert!(xml.contains("network='tailnet-android-guest'"));
        assert!(!xml.contains("authkey"));
        assert!(!xml.contains("tailscale0"));
    }
}
