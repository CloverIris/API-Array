use crate::{CoreError, provider::ProviderManifest};

/// Built-in provider manifests shipped with the application.
///
/// The catalog is deliberately compiled into the binary: the first release does
/// not download or execute provider definitions from the network.
pub const BUILTIN_PROVIDER_IDS: &[&str] = &[
    "openai-official",
    "anthropic-official",
    "google-gemini-official",
    "custom-openai-compatible",
];

pub fn builtin_provider_manifests() -> Result<Vec<ProviderManifest>, CoreError> {
    [
        include_str!("../../../providers/openai.yaml"),
        include_str!("../../../providers/anthropic.yaml"),
        include_str!("../../../providers/google-gemini.yaml"),
        include_str!("../../../providers/custom-openai-compatible.yaml"),
    ]
    .into_iter()
    .map(ProviderManifest::from_yaml)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ships_the_four_first_release_provider_manifests() -> Result<(), CoreError> {
        let manifests = builtin_provider_manifests()?;
        assert_eq!(manifests.len(), BUILTIN_PROVIDER_IDS.len());
        assert!(
            manifests
                .iter()
                .all(|manifest| { BUILTIN_PROVIDER_IDS.contains(&manifest.provider.id.as_str()) })
        );
        Ok(())
    }
}
