use crate::config::Config;
use std::path::PathBuf;

#[must_use]
pub fn disk_path(config: &Config) -> PathBuf {
    config.storage.vm_dir.join("tailnet-router.qcow2")
}

#[must_use]
pub fn domain_xml(config: &Config) -> String {
    let disk = disk_path(config);
    let disk = xml_attribute(&disk.display().to_string());
    let bridge = xml_attribute(&config.network.libvirt_bridge);
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
    <interface type='bridge'>
      <source bridge='{bridge}'/>
      <model type='virtio'/>
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
    fn router_has_one_private_nic_and_no_secret() {
        let c = crate::config::tests::valid();
        let xml = domain_xml(&c);
        assert_eq!(xml.matches("<interface").count(), 1);
        assert!(xml.contains("bridge='vmbr-android'"));
        assert!(!xml.contains("authkey"));
        assert!(!xml.contains("tailscale0"));
    }
}
