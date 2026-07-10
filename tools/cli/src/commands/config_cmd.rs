//! `corelink config` subcommand handler (WI-S15-001).
//!
//! Manages `~/.corelink/config.toml` via `set`, `get`, and `list` actions.

use crate::config;
use crate::error::CliError;
use crate::output::OutputFormat;

/// Run `corelink config set <key> <value>`.
pub fn run_set(key: &str, value: &str, _format: OutputFormat) -> Result<(), CliError> {
    config::set_key(key, value)?;
    println!("Config updated: {key} = {value}");
    Ok(())
}

/// Run `corelink config get <key>`.
pub fn run_get(key: &str, _format: OutputFormat) -> Result<(), CliError> {
    let value = config::get_key(key)?;
    println!("{key} = {value}");
    Ok(())
}

/// Run `corelink config list`.
pub fn run_list(_format: OutputFormat) -> Result<(), CliError> {
    let sanitised = config::list_sanitised()?;
    println!("{sanitised}");
    Ok(())
}

/// Run `corelink config apply --file <path>`.
///
/// Loads a whole TOML document (e.g. the `corelink.toml` from the
/// observability how-tos) and applies every leaf key through the same
/// validation the `config set` path uses. Writes the merged result back
/// to `~/.corelink/config.toml`.
pub fn run_apply(file: &std::path::Path, _format: OutputFormat) -> Result<(), CliError> {
    let count = config::apply_file(file)?;
    println!(
        "Config applied: {count} key(s) from {} → {}",
        file.display(),
        config::config_path().display()
    );
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use crate::config::{
        apply_key_to_cfg, list_sanitised_from_cfg, load_from_path, save_to_path, CorelinkConfig,
    };

    #[test]
    fn config_set_and_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".corelink").join("config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // Start with empty config, set defaults.tenant_id.
        let mut cfg = CorelinkConfig::default();
        apply_key_to_cfg(&mut cfg, "defaults.tenant_id", "acme-test").unwrap();
        save_to_path(&path, &cfg).unwrap();

        // Reload and check.
        let loaded = load_from_path(&path).unwrap();
        assert_eq!(loaded.defaults.tenant_id.as_deref(), Some("acme-test"));
    }

    #[test]
    fn config_list_redacts_pat() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".corelink").join("config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut cfg = CorelinkConfig::default();
        cfg.auth.pat = Some(
            "corelink_pat_ABCDEFGH01234567.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.AAAAAAAAAAAAAAAAAAAAAA"
                .to_owned(),
        );
        save_to_path(&path, &cfg).unwrap();

        let listing = list_sanitised_from_cfg(&cfg);
        // Should NOT contain the full secret portion.
        assert!(
            !listing.contains("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            "PAT not redacted: {listing}"
        );
        // Should contain redacted form.
        assert!(listing.contains("***"), "No *** in listing: {listing}");
    }

    #[test]
    fn unknown_key_returns_error() {
        let mut cfg = CorelinkConfig::default();
        let result = apply_key_to_cfg(&mut cfg, "nonexistent.key", "val");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_output_format_rejected() {
        let mut cfg = CorelinkConfig::default();
        let result = apply_key_to_cfg(&mut cfg, "defaults.output_format", "xml");
        assert!(result.is_err());
    }
}
