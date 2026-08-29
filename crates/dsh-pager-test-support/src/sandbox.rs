use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Owns all filesystem and environment state used by a test.
#[derive(Debug)]
pub struct TestSandbox {
    temp: PathBuf,
    root: PathBuf,
    home: PathBuf,
    workspace: PathBuf,
    tmp: PathBuf,
    environment: BTreeMap<OsString, OsString>,
}

impl TestSandbox {
    pub fn new() -> io::Result<Self> {
        let base = std::env::temp_dir();
        let process = std::process::id();
        let mut root = None;
        for attempt in 0..100u32 {
            let candidate = base.join(format!("dsh-pager-test-{process}-{attempt}"));
            match std::fs::create_dir(&candidate) {
                Ok(()) => {
                    root = Some(candidate);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        let root = root.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique dsh-pager test sandbox",
            )
        })?;
        let home = root.join("home");
        let workspace = root.join("workspace");
        let tmp = root.join("tmp");
        std::fs::create_dir_all(&home)?;
        std::fs::create_dir_all(&workspace)?;
        std::fs::create_dir_all(&tmp)?;

        let mut environment = BTreeMap::new();
        // Preserve only executable discovery. Tests may add explicit values,
        // but never inherit credentials or ambient DSH server configuration.
        if let Some(path) = std::env::var_os("PATH") {
            environment.insert("PATH".into(), path);
        }
        // `env_clear` also removes SYSTEMROOT on Windows. System services may
        // expand `%SYSTEMROOT%` while lazily loading DLLs, so runtimes such as
        // Node can fail during crypto initialization before user code starts.
        // Keep this platform bootstrap value without inheriting user config,
        // credentials, or ambient DSH variables.
        #[cfg(windows)]
        {
            let system_root = std::env::var_os("SYSTEMROOT").ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "SYSTEMROOT is required for a usable Windows test sandbox",
                )
            })?;
            environment.insert("SYSTEMROOT".into(), system_root);
        }
        // Isolate Unix HOME/TMPDIR and Windows USERPROFILE/TMP/TEMP. Key set
        // matches grok-build xai-grok-test-support sandbox.rs; DSH does not set
        // GROK_HOME or Grok telemetry/git hermetic env.
        environment.insert("HOME".into(), home.clone().into_os_string());
        environment.insert("USERPROFILE".into(), home.clone().into_os_string());
        environment.insert("TMPDIR".into(), tmp.clone().into_os_string());
        environment.insert("TMP".into(), tmp.clone().into_os_string());
        environment.insert("TEMP".into(), tmp.clone().into_os_string());
        environment.insert("DSH_TEST_SANDBOX".into(), root.clone().into_os_string());
        environment.insert("NO_COLOR".into(), OsString::from("1"));
        environment.insert("LC_ALL".into(), OsString::from("C"));
        Ok(Self {
            temp: root.clone(),
            root,
            home,
            workspace,
            tmp,
            environment,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn tmp(&self) -> &Path {
        &self.tmp
    }

    pub fn set_env(&mut self, key: impl Into<OsString>, value: impl Into<OsString>) {
        self.environment.insert(key.into(), value.into());
    }

    pub fn remove_env(&mut self, key: impl AsRef<OsStr>) {
        self.environment.remove(key.as_ref());
    }

    pub fn env(&self) -> &BTreeMap<OsString, OsString> {
        &self.environment
    }

    /// Build a hermetic command with the sandbox cwd and environment.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .envs(self.environment.iter())
            .current_dir(&self.workspace);
        command
    }

    /// Keep the owner alive explicitly in tests that need to inspect paths
    /// after a child exits. The sandbox removes its private root on drop.
    pub fn keep_alive(&self) -> &Path {
        &self.temp
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.temp);
    }
}

impl Default for TestSandbox {
    fn default() -> Self {
        Self::new().expect("create test sandbox")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_value(sandbox: &TestSandbox, key: &str) -> Option<OsString> {
        sandbox.env().get(OsStr::new(key)).cloned()
    }

    #[test]
    fn sandbox_has_isolated_directories_and_no_ambient_backend() {
        let sandbox = TestSandbox::new().expect("sandbox");
        assert!(sandbox.home().is_dir());
        assert!(sandbox.workspace().is_dir());
        assert!(sandbox.tmp().is_dir());
        assert!(!sandbox.env().contains_key(OsStr::new("DSH_TUI_SERVER")));
    }

    #[test]
    fn cross_platform_home_and_temp_names_are_present() {
        let sandbox = TestSandbox::new().expect("sandbox");
        assert_eq!(
            env_value(&sandbox, "USERPROFILE"),
            Some(sandbox.home().into())
        );
        assert_eq!(env_value(&sandbox, "HOME"), Some(sandbox.home().into()));
        assert_eq!(env_value(&sandbox, "TEMP"), Some(sandbox.tmp().into()));
        assert_eq!(env_value(&sandbox, "TMP"), Some(sandbox.tmp().into()));
        assert_eq!(env_value(&sandbox, "TMPDIR"), Some(sandbox.tmp().into()));
    }

    #[cfg(windows)]
    #[test]
    fn windows_systemroot_is_preserved_for_child_runtime_support() {
        let expected = std::env::var_os("SYSTEMROOT").expect("parent SYSTEMROOT");
        let sandbox = TestSandbox::new().expect("sandbox");
        assert_eq!(env_value(&sandbox, "SYSTEMROOT"), Some(expected));
    }

    #[cfg(windows)]
    #[test]
    fn windows_sandbox_can_start_node_crypto_runtime() {
        crate::require_node().expect("node is required for Windows sandbox tests");
        let sandbox = TestSandbox::new().expect("sandbox");
        let output = sandbox
            .command("node")
            .args(["-e", "require('node:crypto').randomBytes(1)"])
            .output()
            .expect("start node in sandbox");
        assert!(
            output.status.success(),
            "node crypto runtime failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
