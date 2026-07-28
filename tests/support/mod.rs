use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub const EX_USAGE: i32 = 64;
pub const EX_DATAERR: i32 = 65;
pub const EX_CONFIG: i32 = 78;

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);
static GATE: OnceLock<GateDecision> = OnceLock::new();

#[derive(Clone, Copy, Debug)]
pub enum GateLevel {
    Cli,
    ConfigEval,
    Virtualization,
}

#[derive(Debug)]
pub struct GateDecision {
    viv: PathBuf,
    cli: Result<(), String>,
    config_eval: Result<(), String>,
    virtualization: Result<(), String>,
}

impl GateDecision {
    pub fn viv(&self) -> &Path {
        &self.viv
    }

    pub const fn result(&self, level: GateLevel) -> &Result<(), String> {
        match level {
            GateLevel::Cli => &self.cli,
            GateLevel::ConfigEval => &self.config_eval,
            GateLevel::Virtualization => &self.virtualization,
        }
    }

    pub fn is_well_formed(&self) -> bool {
        !self.viv.as_os_str().is_empty()
            && self
                .result(GateLevel::Cli)
                .as_ref()
                .err()
                .is_none_or(|reason| !reason.is_empty())
            && self
                .result(GateLevel::ConfigEval)
                .as_ref()
                .err()
                .is_none_or(|reason| !reason.is_empty())
            && self
                .result(GateLevel::Virtualization)
                .as_ref()
                .err()
                .is_none_or(|reason| !reason.is_empty())
    }
}

pub fn gate() -> &'static GateDecision {
    GATE.get_or_init(probe_gate)
}

fn probe_gate() -> GateDecision {
    let viv = resolve_viv();
    let cli = probe_cli(&viv);
    let config_eval = cli
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|()| probe_nix());
    let virtualization = config_eval
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|()| probe_kvm());

    GateDecision {
        viv,
        cli,
        config_eval,
        virtualization,
    }
}

fn resolve_viv() -> PathBuf {
    if let Some(path) = std::env::var_os("VIVARIUM_TEST_VIV") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_viv") {
        return PathBuf::from(path);
    }

    let local = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join(format!("viv{}", std::env::consts::EXE_SUFFIX));
    if local.is_file() {
        return local;
    }

    PathBuf::from(format!("viv{}", std::env::consts::EXE_SUFFIX))
}

fn probe_cli(viv: &Path) -> Result<(), String> {
    let tp = TempProject::new().map_err(|error| format!("cannot isolate CLI probe: {error}"))?;
    let out = run_viv(viv, &tp, tp.project(), &["manifest", "list", "--json"])
        .map_err(|error| format!("cannot execute {}: {error}", viv.display()))?;
    if out.status.code() != Some(0) {
        return Err(format!(
            "{} manifest probe exited {:?}",
            viv.display(),
            out.status.code()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout.contains("\"manifests\"") {
        return Err(format!(
            "{} manifest probe did not emit the required manifests key",
            viv.display()
        ));
    }
    Ok(())
}

fn probe_nix() -> Result<(), String> {
    match Command::new("nix").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!("nix --version exited {:?}", output.status.code())),
        Err(error) => Err(format!("nix --version is unavailable: {error}")),
    }
}

fn probe_kvm() -> Result<(), String> {
    let path = Path::new("/dev/kvm");
    if !path.exists() {
        return Err("/dev/kvm is absent".to_owned());
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map(drop)
        .map_err(|error| format!("/dev/kvm is not read-write openable: {error}"))
}

#[derive(Debug)]
pub struct TempProject {
    root: PathBuf,
    project: PathBuf,
    home: PathBuf,
    config: PathBuf,
    state: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    runtime: PathBuf,
}

impl TempProject {
    pub fn new() -> io::Result<Self> {
        Self::with_project_name("project")
    }

    pub fn with_project_name(name: &str) -> io::Result<Self> {
        let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vivarium-user-workflows-{}-{sequence}",
            std::process::id()
        ));
        let project = root.join(name);
        let home = root.join("home");
        let config = root.join("xdg-config");
        let state = root.join("xdg-state");
        let data = root.join("xdg-data");
        let cache = root.join("xdg-cache");
        let runtime = root.join("xdg-runtime");
        for path in [&project, &home, &config, &state, &data, &cache, &runtime] {
            fs::create_dir_all(path)?;
        }
        let project = project.canonicalize()?;
        Ok(Self {
            root,
            project,
            home,
            config,
            state,
            data,
            cache,
            runtime,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn state(&self) -> &Path {
        &self.state
    }

    pub fn data(&self) -> &Path {
        &self.data
    }

    pub fn cache(&self) -> &Path {
        &self.cache
    }

    pub fn runtime(&self) -> &Path {
        &self.runtime
    }

    pub fn env(&self) -> Vec<(OsString, OsString)> {
        [
            ("HOME", self.home()),
            ("XDG_CONFIG_HOME", self.config()),
            ("XDG_STATE_HOME", self.state()),
            ("XDG_DATA_HOME", self.data()),
            ("XDG_CACHE_HOME", self.cache()),
            ("XDG_RUNTIME_DIR", self.runtime()),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value.as_os_str().to_owned()))
        .collect()
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _result = fs::remove_dir_all(&self.root);
    }
}

#[derive(Debug)]
pub struct VivOutput {
    pub argv: Vec<String>,
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The only host variables re-injected after `env_clear`. Everything else is dropped,
/// which is what makes the fail-closed assertions honest: an ambient `VIVARIUM_MANIFEST`
/// sits second in the manifest resolution order (spec/01), so a leaked one would silently
/// turn every expected `78` into a success.
const HOST_PASSTHROUGH: [&str; 6] = ["PATH", "TERM", "LANG", "LC_ALL", "SSL_CERT_FILE", "TMPDIR"];

pub fn run_viv(bin: &Path, tp: &TempProject, cwd: &Path, args: &[&str]) -> io::Result<VivOutput> {
    let command_line = std::iter::once(bin.display().to_string())
        .chain(args.iter().map(|arg| (*arg).to_owned()))
        .collect();
    let mut command = Command::new(bin);
    command.args(args).current_dir(cwd).env_clear();
    for name in HOST_PASSTHROUGH {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    // `nix` needs its own ambient configuration to run at all; it carries no manifest
    // resolution meaning, so passing the family through is safe.
    for (name, value) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("NIX_") {
            command.env(name, value);
        }
    }
    command.envs(tp.env());
    let output = command.output()?;
    Ok(VivOutput {
        argv: command_line,
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub fn expect_code(out: &VivOutput, code: i32) -> Result<(), String> {
    if out.status.code() == Some(code) {
        return Ok(());
    }
    Err(diagnostic(out, &format!("expected exit code {code}")))
}

pub fn expect_stderr_mentions(out: &VivOutput, needle: &str) -> Result<(), String> {
    if String::from_utf8_lossy(&out.stderr).contains(needle) {
        return Ok(());
    }
    Err(diagnostic(
        out,
        &format!("expected stderr to mention {needle:?}"),
    ))
}

pub fn expect_stdout_mentions(out: &VivOutput, needle: &str) -> Result<(), String> {
    if String::from_utf8_lossy(&out.stdout).contains(needle) {
        return Ok(());
    }
    Err(diagnostic(
        out,
        &format!("expected stdout to mention {needle:?}"),
    ))
}

/// The negative half of `expect_stdout_mentions`. Provenance assertions need both:
/// a merged view must carry the winning value and must **not** carry the shadowed one.
pub fn expect_stdout_lacks(out: &VivOutput, needle: &str) -> Result<(), String> {
    if String::from_utf8_lossy(&out.stdout).contains(needle) {
        return Err(diagnostic(
            out,
            &format!("expected stdout to omit {needle:?}"),
        ));
    }
    Ok(())
}

pub fn expect_json_keys(out: &VivOutput, keys: &[&str]) -> Result<(), String> {
    // TODO(green): tighten these coarse shape checks with serde_json once the CLI gate opens.
    let stdout = String::from_utf8_lossy(&out.stdout);
    if keys
        .iter()
        .all(|key| stdout.contains(&format!("\"{key}\"")))
    {
        return Ok(());
    }
    Err(diagnostic(out, &format!("expected JSON keys {keys:?}")))
}

pub fn expect_nonzero(out: &VivOutput) -> Result<(), String> {
    if out.status.code().is_some_and(|code| code != 0) {
        return Ok(());
    }
    Err(diagnostic(out, "expected a non-zero exit status"))
}

pub fn write_file(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeSnapshot(Vec<(PathBuf, Option<Vec<u8>>)>);

pub fn snapshot_tree(root: &Path) -> io::Result<TreeSnapshot> {
    fn visit(
        base: &Path,
        current: &Path,
        entries: &mut Vec<(PathBuf, Option<Vec<u8>>)>,
    ) -> io::Result<()> {
        let mut children = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let path = child.path();
            let relative = path
                .strip_prefix(base)
                .map_err(io::Error::other)?
                .to_path_buf();
            if path.is_dir() {
                entries.push((relative, None));
                visit(base, &path, entries)?;
            } else {
                entries.push((relative, Some(fs::read(path)?)));
            }
        }
        Ok(())
    }

    let mut entries = Vec::new();
    visit(root, root, &mut entries)?;
    Ok(TreeSnapshot(entries))
}

pub fn expect_tree_unchanged(root: &Path, before: &TreeSnapshot) -> Result<(), String> {
    let after = snapshot_tree(root).map_err(|error| error.to_string())?;
    if &after == before {
        Ok(())
    } else {
        Err(format!(
            "project tree changed\nbefore: {before:?}\nafter: {after:?}"
        ))
    }
}

pub fn expect_no_project_binding_files(project: &Path) -> Result<(), String> {
    for name in [".vivarium.toml", ".vivarium.local.toml"] {
        if project.join(name).exists() {
            return Err(format!(
                "obsolete project binding file exists: {}",
                project.join(name).display()
            ));
        }
    }
    Ok(())
}

pub fn expect_marker_id(project: &Path, expected: &str) -> Result<(), String> {
    let marker = project.join(".vivarium");
    let ignore =
        fs::read_to_string(marker.join(".gitignore")).map_err(|error| error.to_string())?;
    let id = fs::read_to_string(marker.join("id")).map_err(|error| error.to_string())?;
    if ignore.trim_end() != "*" || id != format!("{expected}\n") {
        return Err(format!(
            "invalid marker: .gitignore={ignore:?}, id={id:?}, expected id={expected:?}"
        ));
    }
    Ok(())
}

pub fn expect_marker_absent(project: &Path) -> Result<(), String> {
    if project.join(".vivarium").exists() {
        Err(format!(
            "identity marker still exists: {}",
            project.join(".vivarium").display()
        ))
    } else {
        Ok(())
    }
}

pub fn expect_registry_binding_visible(out: &VivOutput, manifest: &str) -> Result<(), String> {
    expect_code(out, 0)?;
    expect_json_keys(
        out,
        &[
            "manifest", "source", "paths", "config", "state", "data", "cache",
        ],
    )?;
    expect_stdout_mentions(out, manifest)?;
    expect_stdout_mentions(out, "registry")
}

pub fn expect_no_volume_images(tp: &TempProject) -> Result<(), String> {
    let state = tp.state().join("vivarium").join("projects");
    if !state.exists() {
        return Ok(());
    }
    let mut pending = vec![state];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let child = entry.path();
            if child.is_dir() {
                pending.push(child);
            } else if child
                .extension()
                .is_some_and(|extension| extension == "img")
            {
                return Err(format!("read-only command created {}", child.display()));
            }
        }
    }
    Ok(())
}

pub fn volume_image(tp: &TempProject, project_id: &str, name: &str) -> PathBuf {
    tp.state()
        .join("vivarium")
        .join("projects")
        .join(project_id)
        .join("default")
        .join("volumes")
        .join(format!("{name}.img"))
}

pub fn expect_volume_image(tp: &TempProject, project_id: &str, name: &str) -> Result<(), String> {
    let path = volume_image(tp, project_id, name);
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("volume image is absent: {}", path.display()))
    }
}

pub fn diagnostic(out: &VivOutput, message: &str) -> String {
    format!(
        "{message}\nargv: {:?}\nexit: {:?}\nstdout: {}\nstderr: {}",
        out.argv,
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
