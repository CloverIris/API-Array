use apiarray_core::provider::ProviderManifest;

const PROVIDERS: [(&str, &str); 4] = [
    ("openai", include_str!("../../../providers/openai.yaml")),
    (
        "anthropic",
        include_str!("../../../providers/anthropic.yaml"),
    ),
    (
        "google-gemini",
        include_str!("../../../providers/google-gemini.yaml"),
    ),
    (
        "custom-openai-compatible",
        include_str!("../../../providers/custom-openai-compatible.yaml"),
    ),
];

#[test]
fn all_builtin_providers_are_valid_and_unique() -> Result<(), Box<dyn std::error::Error>> {
    let mut ids = std::collections::HashSet::new();
    for (file_name, yaml) in PROVIDERS {
        let manifest =
            ProviderManifest::from_yaml(yaml).map_err(|error| format!("{file_name}: {error}"))?;
        assert!(
            ids.insert(manifest.provider.id),
            "duplicate provider id in {file_name}"
        );
    }
    Ok(())
}
