use crate::config::{AndroidVmConfig, Config};
use std::path::PathBuf;

#[must_use]
pub fn disk_path(config: &Config, vm: &AndroidVmConfig) -> PathBuf {
    config.storage.vm_dir.join(format!("{}.qcow2", vm.name))
}

#[must_use]
pub fn mac_address(vm: &AndroidVmConfig) -> String {
    let octets = vm.address.octets();
    format!(
        "52:54:00:{:02x}:{:02x}:{:02x}",
        octets[1], octets[2], octets[3]
    )
}

#[must_use]
pub fn domain_xml(config: &Config, vm: &AndroidVmConfig) -> String {
    let disk = xml_attribute(&disk_path(config, vm).display().to_string());
    let network = xml_attribute(&config.network.guest_network);
    let name = xml_text(&vm.name);
    let mac = mac_address(vm);
    format!(
        r#"<domain type='kvm'>
  <name>{name}</name>
  <memory unit='MiB'>{memory}</memory>
  <currentMemory unit='MiB'>{memory}</currentMemory>
  <vcpu placement='static'>{vcpus}</vcpu>
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
    <interface type='network'>
      <mac address='{mac}'/>
      <source network='{network}'/>
      <port isolated='yes'/>
      <model type='virtio'/>
    </interface>
    <graphics type='spice' autoport='yes' listen='127.0.0.1'>
      <listen type='address' address='127.0.0.1'/>
    </graphics>
    <video><model type='virtio' heads='1' primary='yes'/></video>
    <input type='tablet' bus='usb'/>
    <serial type='pty'><target type='isa-serial' port='0'/></serial>
    <console type='pty'><target type='serial' port='0'/></console>
  </devices>
</domain>
"#,
        memory = vm.memory_mib,
        vcpus = vm.vcpus,
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
    fn android_domain_is_persistent_and_port_isolated() {
        let config = crate::config::tests::valid();
        let vm = &config.android_vms[0];
        let xml = domain_xml(&config, vm);
        assert!(xml.contains("android-game-01.qcow2"));
        assert!(xml.contains("network='tailnet-android-guest'"));
        assert!(xml.contains("<port isolated='yes'/>"));
        assert!(xml.contains("listen='127.0.0.1'"));
        assert_eq!(mac_address(vm), "52:54:00:50:00:02");
    }
}
