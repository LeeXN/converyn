//! One configuration resolver for every API command; local service configs stay explicit.
use super::*;
use converyn_runtime::interface::{
    client::Config,
    profile::{self, ContextConfig, Profile},
};
use fs2::FileExt;
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
};

#[derive(Clone, Default)]
pub(crate) struct Source {
    pub file: Option<PathBuf>,
    pub context: Option<String>,
}
impl Source {
    pub fn path(&self) -> Result<PathBuf> {
        resolve_path(
            self.file.clone(),
            std::env::var_os("CONVERYN_CONFIG").map(PathBuf::from),
            std::env::var_os("HOME").map(PathBuf::from),
        )
    }
    pub fn load(&self) -> Result<Config> {
        profile::load(&self.path()?, self.context.as_deref())
    }
    pub fn explicit(path: PathBuf) -> Self {
        Self {
            file: Some(path),
            context: None,
        }
    }
}
fn resolve_path(
    explicit: Option<PathBuf>,
    env: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf> {
    let path = explicit
        .or(env)
        .or_else(|| home.map(|h| h.join(".converyn/config.json")))
        .context("HOME is unset; specify --config PATH")?;
    ensure!(!path.as_os_str().is_empty(), "config path is empty");
    Ok(path)
}
#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Import personal credentials without changing an existing current context.
    Import {
        file: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long = "use")]
        use_context: bool,
        #[arg(long)]
        replace: bool,
    },
    /// List context names, endpoints and the selected context; no credentials.
    GetContexts,
    /// Print the saved current context.
    CurrentContext,
    /// Persist the current context; --context only selects one command temporarily.
    UseContext { name: String },
    /// Print configuration with private keys and tokens redacted.
    View,
    /// Export only the selected context to a new private, self-contained JSON file.
    Export {
        #[arg(long)]
        output: PathBuf,
    },
}
fn private_parent(path: &Path) -> Result<()> {
    let parent = path.parent().context("config parent missing")?;
    if !parent.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    converyn_runtime::credentials::security::private_state_directory(parent)?;
    ensure!(
        parent.canonicalize()? == parent,
        "config directory must not traverse symlinks"
    );
    Ok(())
}
fn lock(path: &Path) -> Result<File> {
    private_parent(path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path.with_extension("json.lock"))?;
    let metadata = lock.metadata()?;
    // SAFETY: geteuid only reads the calling process identity.
    ensure!(
        metadata.is_file()
            && metadata.mode() & 0o077 == 0
            && metadata.uid() == unsafe { libc::geteuid() },
        "config lock must be private and owned by the current user"
    );
    lock.lock_exclusive()?;
    Ok(lock)
}
fn write_profile(path: &Path, profile: &Profile) -> Result<()> {
    profile.validate()?;
    let temporary = path.with_extension(format!(
        "json.{}",
        converyn_runtime::credentials::pki::random_token()?
    ));
    converyn_runtime::credentials::pki::private(&temporary, &serde_json::to_vec_pretty(profile)?)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(temporary);
        return Err(error.into());
    }
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn import(
    path: &Path,
    input: &Path,
    source_context: Option<&str>,
    name: &str,
    use_context: bool,
    replace: bool,
) -> Result<Profile> {
    ensure!(profile::valid_name(name), "invalid context name");
    let context = ContextConfig::from_config(profile::load(input, source_context)?)?;
    let _lock = lock(path)?;
    let mut profile = if path.try_exists()? {
        profile::read_profile(path)?
    } else {
        Profile::default()
    };
    ensure!(
        replace || !profile.contexts.contains_key(name),
        "context already exists; use --replace explicitly"
    );
    profile.contexts.insert(name.into(), context);
    if use_context || profile.current_context.is_none() {
        profile.current_context = Some(name.into());
    }
    write_profile(path, &profile)?;
    Ok(profile)
}
pub(crate) fn run(source: Source, command: ConfigCommands) -> Result<()> {
    let path = std::path::absolute(source.path()?)?;
    match command {
        ConfigCommands::Import {
            file,
            name,
            use_context,
            replace,
        } => {
            let profile = import(
                &path,
                &file,
                source.context.as_deref(),
                &name,
                use_context,
                replace,
            )?;
            output::print(
                &json!({"imported":name,"current_context":profile.current_context,"config":path}),
            )?;
        }
        ConfigCommands::GetContexts => {
            let profile = profile::read_profile(&path)?;
            let contexts=profile.contexts.iter().map(|(name,c)|json!({"name":name,"cluster":c.cluster,"servers":c.servers,"current":profile.current_context.as_deref()==Some(name)})).collect::<Vec<_>>();
            output::print(&json!({"current_context":profile.current_context,"contexts":contexts}))?;
        }
        ConfigCommands::CurrentContext => {
            let profile = profile::read_profile(&path)?;
            output::print(&profile.current_context.context("no current context")?)?;
        }
        ConfigCommands::UseContext { name } => {
            ensure!(
                source.context.is_none(),
                "use-context takes NAME; --context is for temporary selection"
            );
            let _lock = lock(&path)?;
            let mut profile = profile::read_profile(&path)?;
            profile.select(Some(&name))?;
            profile.current_context = Some(name.clone());
            write_profile(&path, &profile)?;
            output::print(&name)?;
        }
        ConfigCommands::View => {
            let mut profile = profile::read_profile(&path)?;
            for context in profile.contexts.values_mut() {
                context.credentials.private_key_pem = "[REDACTED]".into();
                context.credentials.token = "[REDACTED]".into();
            }
            output::print(&profile)?;
        }
        ConfigCommands::Export { output } => {
            let profile = profile::read_profile(&path)?;
            let (name, context) = profile.select(source.context.as_deref())?;
            let exported = Profile {
                version: 1,
                current_context: Some(name.into()),
                contexts: std::collections::BTreeMap::from([(name.into(), context.clone())]),
            };
            let output = std::path::absolute(output)?;
            // Export never overwrites another context file, credential, or symlink.
            converyn_runtime::credentials::pki::private(
                &output,
                &serde_json::to_vec_pretty(&exported)?,
            )?;
            output::print(&json!({"exported":name,"output":output}))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn personal_profile(root: &Path, name: &str) -> PathBuf {
        let bundle = root.join(name);
        let ca = root.join(format!("{name}-ca"));
        let authority = converyn_runtime::credentials::pki::create_authority(&ca, name).unwrap();
        authority.issue(&bundle, "personal", &[]).unwrap();
        let private = converyn_runtime::credentials::pki::private;
        private(
            &bundle.join("ca.crt"),
            &std::fs::read(&authority.cert).unwrap(),
        )
        .unwrap();
        private(&bundle.join("token"), b"0123456789abcdef0123456789abcdef").unwrap();
        converyn_runtime::interface::profile::write(
            &bundle.join("config.json"),
            name,
            converyn_runtime::interface::client::Config {
                cluster: name.into(),
                servers: vec!["https://127.0.0.1:8443".into()],
                token_file: bundle.join("token"),
                tls: converyn_runtime::credentials::security::Tls {
                    cert: bundle.join("tls.crt"),
                    key: bundle.join("tls.key"),
                    ca: bundle.join("ca.crt"),
                },
                embedded: None,
            },
        )
        .unwrap();
        bundle.join("config.json")
    }
    #[test]
    fn precedence_does_not_fall_back_for_explicit_missing_path() {
        let path = resolve_path(
            Some("missing".into()),
            Some("env".into()),
            Some("/home/op".into()),
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("missing"));
        assert_eq!(
            resolve_path(None, Some("env".into()), None).unwrap(),
            PathBuf::from("env")
        );
        assert_eq!(
            resolve_path(None, None, Some("/home/op".into())).unwrap(),
            PathBuf::from("/home/op/.converyn/config.json")
        );
        assert!(resolve_path(Some(PathBuf::new()), Some("env".into()), None).is_err());
    }
    #[test]
    fn portable_import_conflict_selection_and_private_export() {
        let root = tempfile::tempdir().unwrap();
        let first = personal_profile(root.path(), "single");
        let second = personal_profile(root.path(), "aio");
        let target = root.path().join("managed/config.json");
        let profile = import(&target, &first, None, "single", false, false).unwrap();
        assert_eq!(profile.current_context.as_deref(), Some("single"));
        let profile = import(&target, &second, None, "aio", false, false).unwrap();
        assert_eq!(profile.current_context.as_deref(), Some("single"));
        assert!(import(&target, &second, None, "single", true, false).is_err());
        let profile = import(&target, &second, None, "aio", true, true).unwrap();
        assert_eq!(profile.current_context.as_deref(), Some("aio"));
        // Removing all source files proves that the managed config is self-contained.
        std::fs::remove_dir_all(root.path().join("single")).unwrap();
        std::fs::remove_dir_all(root.path().join("aio")).unwrap();
        assert_eq!(
            profile::load(&target, Some("single")).unwrap().cluster,
            "single"
        );
        assert_eq!(profile::load(&target, None).unwrap().cluster, "aio");
        assert!(profile::load(&target, Some("missing")).is_err());
        let exported = root.path().join("export.json");
        run(
            Source::explicit(target.clone()),
            ConfigCommands::Export {
                output: exported.clone(),
            },
        )
        .unwrap();
        let portable = profile::read_profile(&exported).unwrap();
        assert_eq!(portable.contexts.len(), 1);
        assert!(portable.contexts.contains_key("aio"));
        assert_eq!(
            exported.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            run(
                Source::explicit(target.clone()),
                ConfigCommands::Export { output: exported }
            )
            .is_err()
        );
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(profile::load(&target, None).is_err());
    }
}
