use crate::config::{AndroidVmConfig, Config};
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selector {
    pub name: Option<String>,
    pub all: bool,
    pub labels: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SelectorError {
    #[error("select exactly one of NAME, --all, or one or more --label values")]
    Ambiguous,
    #[error("Android VM is not configured: {0}")]
    UnknownVm(String),
    #[error("selector matched no Android VMs")]
    Empty,
}

pub fn select<'a>(
    config: &'a Config,
    selector: &Selector,
) -> Result<Vec<&'a AndroidVmConfig>, SelectorError> {
    let modes = usize::from(selector.name.is_some())
        + usize::from(selector.all)
        + usize::from(!selector.labels.is_empty());
    if modes != 1 {
        return Err(SelectorError::Ambiguous);
    }
    if let Some(name) = &selector.name {
        return config
            .android_vms
            .iter()
            .find(|vm| vm.name == *name)
            .map(|vm| vec![vm])
            .ok_or_else(|| SelectorError::UnknownVm(name.clone()));
    }
    let selected = config
        .android_vms
        .iter()
        .filter(|vm| {
            selector.all
                || selector
                    .labels
                    .iter()
                    .all(|label| vm.labels.contains(label))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        Err(SelectorError::Empty)
    } else {
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_intersection_of_labels() {
        let mut config = crate::config::tests::valid();
        config.android_vms[0].labels = vec!["game".into(), "primary".into()];
        let selected = select(
            &config,
            &Selector {
                labels: vec!["game".into(), "primary".into()],
                ..Selector::default()
            },
        )
        .unwrap();
        assert_eq!(selected[0].name, "android-game-01");
    }

    #[test]
    fn rejects_ambiguous_selector() {
        let config = crate::config::tests::valid();
        assert_eq!(
            select(
                &config,
                &Selector {
                    name: Some("android-game-01".into()),
                    all: true,
                    labels: vec![],
                }
            ),
            Err(SelectorError::Ambiguous)
        );
    }
}
