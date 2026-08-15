use crate::config::Config;
use nftables::{helper, schema::Nftables};
use serde_json::{Value, json};
use std::process::Command;
use thiserror::Error;

const FAMILY: &str = "inet";
const TABLE: &str = "tailnet_android_router";
const ALLOWED_SET: &str = "allowed_adb_flows";

#[derive(Debug, Error)]
pub enum FirewallError {
    #[error("could not inspect the existing nftables table: {0}")]
    Inspect(#[from] std::io::Error),
    #[error("generated nftables JSON did not match the typed schema: {0}")]
    Schema(#[from] serde_json::Error),
    #[error("nftables rejected the generated policy: {0}")]
    Apply(#[from] helper::NftablesError),
}

pub fn ruleset(config: &Config, replace: bool) -> Result<Nftables<'static>, FirewallError> {
    let mut commands = Vec::new();
    if replace {
        commands.push(json!({"delete": {"table": {
            "family": FAMILY, "name": TABLE
        }}}));
    }
    commands.push(json!({"add": {"table": {
        "family": FAMILY, "name": TABLE
    }}}));

    let elements = config
        .router
        .access
        .iter()
        .flat_map(|access| {
            access
                .sources
                .iter()
                .map(|source| json!({"concat": [source.to_string(), access.guest.to_string()]}))
        })
        .collect::<Vec<_>>();
    commands.push(json!({"add": {"set": {
        "family": FAMILY,
        "table": TABLE,
        "name": ALLOWED_SET,
        "type": ["ipv4_addr", "ipv4_addr"],
        "elem": elements,
        "comment": "controller source and Android guest pairs"
    }}}));
    commands.push(json!({"add": {"chain": {
        "family": FAMILY,
        "table": TABLE,
        "name": "forward",
        "type": "filter",
        "hook": "forward",
        "prio": 0,
        "policy": "drop"
    }}}));
    commands.push(json!({"add": {"chain": {
        "family": FAMILY,
        "table": TABLE,
        "name": "postrouting",
        "type": "nat",
        "hook": "postrouting",
        "prio": 100,
        "policy": "accept"
    }}}));

    commands.push(rule(vec![
        meta_match("iifname", &config.router.tailscale_interface),
        meta_match("oifname", &config.router.guest_interface),
        json!({"match": {
            "op": "in",
            "left": {"concat": [
                {"payload": {"protocol": "ip", "field": "saddr"}},
                {"payload": {"protocol": "ip", "field": "daddr"}}
            ]},
            "right": format!("@{ALLOWED_SET}")
        }}),
        payload_match("tcp", "dport", json!(5555)),
        json!({"accept": null}),
    ]));
    commands.push(rule(vec![
        meta_match("iifname", &config.router.guest_interface),
        meta_match("oifname", &config.router.tailscale_interface),
        payload_match(
            "ip",
            "saddr",
            prefix(&config.network.guest_subnet.to_string()),
        ),
        conntrack_established(),
        json!({"accept": null}),
    ]));
    commands.push(rule(vec![
        meta_match("iifname", &config.router.guest_interface),
        meta_match("oifname", &config.router.uplink_interface),
        payload_match(
            "ip",
            "saddr",
            prefix(&config.network.guest_subnet.to_string()),
        ),
        json!({"accept": null}),
    ]));
    commands.push(rule(vec![
        meta_match("iifname", &config.router.uplink_interface),
        meta_match("oifname", &config.router.guest_interface),
        payload_match(
            "ip",
            "daddr",
            prefix(&config.network.guest_subnet.to_string()),
        ),
        conntrack_established(),
        json!({"accept": null}),
    ]));
    commands.push(json!({"add": {"rule": {
        "family": FAMILY,
        "table": TABLE,
        "chain": "postrouting",
        "expr": [
            meta_match("oifname", &config.router.uplink_interface),
            payload_match("ip", "saddr", prefix(&config.network.guest_subnet.to_string())),
            json!({"masquerade": null})
        ]
    }}}));

    let document = json!({"nftables": commands});
    Ok(serde_json::from_value(document)?)
}

pub fn render(config: &Config, replace: bool) -> Result<String, FirewallError> {
    Ok(serde_json::to_string_pretty(&ruleset(config, replace)?)?)
}

pub fn apply(config: &Config) -> Result<(), FirewallError> {
    let exists = Command::new("nft")
        .args(["list", "table", FAMILY, TABLE])
        .output()?
        .status
        .success();
    helper::apply_ruleset(&ruleset(config, exists)?)?;
    Ok(())
}

fn rule(expr: Vec<Value>) -> Value {
    json!({"add": {"rule": {
        "family": FAMILY, "table": TABLE, "chain": "forward", "expr": expr
    }}})
}

fn meta_match(key: &str, value: &str) -> Value {
    json!({"match": {
        "op": "==", "left": {"meta": {"key": key}}, "right": value
    }})
}

fn payload_match(protocol: &str, field: &str, right: Value) -> Value {
    json!({"match": {
        "op": "==",
        "left": {"payload": {"protocol": protocol, "field": field}},
        "right": right
    }})
}

fn conntrack_established() -> Value {
    json!({"match": {
        "op": "in",
        "left": {"ct": {"key": "state"}},
        "right": ["established", "related"]
    }})
}

fn prefix(network: &str) -> Value {
    let (address, length) = network.split_once('/').expect("validated IP network");
    json!({"prefix": {"addr": address, "len": length.parse::<u32>().expect("prefix")}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_typed_multi_controller_set_and_fail_closed_forwarding() {
        let mut config = crate::config::tests::valid();
        config.router.access[0]
            .sources
            .push("100.64.0.3".parse().unwrap());
        let rendered = render(&config, false).unwrap();
        assert!(rendered.contains("allowed_adb_flows"));
        assert!(rendered.contains("100.64.0.2"));
        assert!(rendered.contains("100.64.0.3"));
        assert!(rendered.contains("\"policy\": \"drop\""));
        assert!(rendered.contains("\"masquerade\": null"));
        assert_eq!(ruleset(&config, false).unwrap().objects.len(), 9);
    }
}
