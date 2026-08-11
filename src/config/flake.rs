//! Pure resolution and rendering of the private generated-flake tree.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use super::error::GeneratedFlakeErrorKind;
use super::input::{InputDeclarer, merge_inputs, parse_inputs};
use super::{
    ArtifactForm, ArtifactKind, GeneratedFlakeError, Manifest, ResolvedArtifact, XdgRoots,
    resolve_artifact,
};
use crate::diagnostic::{Locus, Namespace};

const FLAKE_FILE: &str = "flake.nix";
const MANIFEST_SOURCE_FILE: &str = "manifest.toml";
const MANIFEST_MODULE_FILE: &str = "manifest-leaf.nix";
const LOCK_FILE: &str = "flake.lock";

pub use super::input::FlakeInput;

/// All durable paths involved in preparing one target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFlakePaths {
    /// The regenerable generated tree.
    pub directory: PathBuf,
    /// The per-target tool-owned pin.
    pub owned_lock: PathBuf,
    /// The optional per-directory-manifest team pin.
    pub override_lock: Option<PathBuf>,
}

/// The one lock selected for this preparation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EffectiveLock {
    /// A read-only team pin beside a directory-form manifest.
    Override { path: PathBuf },
    /// A tool-owned pin already exists and ordinary build must not move it.
    OwnedExisting { path: PathBuf },
    /// No pin exists yet; a successful first build may install exactly one.
    OwnedMissing { path: PathBuf },
}

impl EffectiveLock {
    /// The selected durable lock path, whether or not it exists yet.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Override { path }
            | Self::OwnedExisting { path }
            | Self::OwnedMissing { path } => path,
        }
    }

    /// Whether a successful build may persist a newly created lock.
    #[must_use]
    pub const fn may_persist_created(&self) -> bool {
        matches!(self, Self::OwnedMissing { .. })
    }
}

/// A validated `extends` target and the one manifest directory it requires.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedExtends {
    pub(super) directory: PathBuf,
    pub(super) relative_target: PathBuf,
    pub(super) manifest_name: String,
}

/// The complete read-only composition closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedComposition {
    /// The selected image.
    pub image: ResolvedArtifact,
    /// The selected pieces, preserving declaration order and duplicates.
    pub pieces: Vec<ResolvedArtifact>,
    /// The bounded manifest module, when present.
    pub extends: Option<ResolvedExtends>,
    /// Artifact-declared inputs sorted by name.
    pub inputs: BTreeMap<String, FlakeInput>,
    /// Paths for generated and durable artifacts.
    pub paths: GeneratedFlakePaths,
    /// The one effective lock decision.
    pub effective_lock: EffectiveLock,
}

/// One operation in the deterministic generated-tree plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeneratedEntry {
    /// Bytes rendered by vivarium.
    RenderedFile {
        destination: PathBuf,
        bytes: Vec<u8>,
    },
    /// One source file copied byte-for-byte.
    CopiedFile {
        source: PathBuf,
        destination: PathBuf,
    },
    /// A complete source directory copied without following symlinks.
    CopiedTree {
        source: PathBuf,
        destination: PathBuf,
    },
}

impl GeneratedEntry {
    pub(super) fn destination(&self) -> &Path {
        match self {
            Self::RenderedFile { destination, .. }
            | Self::CopiedFile { destination, .. }
            | Self::CopiedTree { destination, .. } => destination,
        }
    }
}

/// The complete, inspectable shape to publish.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFlakePlan {
    /// Final generated directory.
    pub directory: PathBuf,
    /// Ordered operations beneath that directory.
    pub entries: Vec<GeneratedEntry>,
    /// The lock decision carried into publication and later persistence.
    pub effective_lock: EffectiveLock,
}

/// A fully published generated flake ready for item 5 to evaluate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedFlake {
    /// The published flake directory.
    pub directory: PathBuf,
    /// The lock selected before publication.
    pub effective_lock: EffectiveLock,
}

impl ResolvedComposition {
    /// Resolves the composition closure and effective lock without writing.
    ///
    /// # Errors
    ///
    /// Returns [`GeneratedFlakeError`] for invalid path components, unresolved artifacts,
    /// malformed declarations, an unbounded `extends`, or a lock probe failure.
    #[allow(clippy::too_many_lines)]
    pub fn resolve(
        roots: &XdgRoots,
        project_id: &str,
        target: &str,
        selected_manifest: &ResolvedArtifact,
        manifest_source: &str,
        manifest: &Manifest,
    ) -> Result<Self, GeneratedFlakeError> {
        validate_component(project_id, "project id")?;
        validate_component(target, "target")?;
        let image = resolve_layer(&roots.config, ArtifactKind::Image, &manifest.image)?;
        let pieces = manifest
            .pieces
            .iter()
            .map(|name| resolve_layer(&roots.config, ArtifactKind::Piece, name))
            .collect::<Result<Vec<_>, _>>()?;

        let mut inputs = BTreeMap::new();
        for artifact in std::iter::once(&image).chain(&pieces) {
            if artifact.form == ArtifactForm::Directory {
                let path = artifact
                    .path
                    .parent()
                    .ok_or_else(|| {
                        GeneratedFlakeError::plain(
                            GeneratedFlakeErrorKind::Internal,
                            Namespace::Internal,
                            "artifact-parent",
                            Locus::File(artifact.path.clone()),
                            "resolved directory artifact has no parent",
                            "the resolver returned an impossible directory form",
                        )
                    })?
                    .join("inputs.toml");
                match fs::read(&path) {
                    // Bytes first, then decoding: a file that reads but does not decode is an
                    // authored defect (`manifest.syntax`, 78), not a failure of the channel
                    // (`manifest.inputs-read`, 74/77). `read_to_string` collapses the two.
                    Ok(bytes) => {
                        let source = decode_inputs(bytes, &path)?;
                        merge_inputs(
                            &mut inputs,
                            parse_inputs(
                                &source,
                                &path,
                                &InputDeclarer {
                                    kind: artifact.kind,
                                    name: artifact.name.clone(),
                                    path: path.clone(),
                                },
                            )
                            .map_err(|error| input_failure(&error))?,
                        )
                        .map_err(|error| input_failure(&error))?;
                    }
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                    Err(source) => {
                        return Err(GeneratedFlakeError::io(
                            Namespace::Manifest,
                            "inputs-read",
                            path.clone(),
                            format!("could not read `{}`", path.display()),
                            source,
                        ));
                    }
                }
            }
        }

        let extends = manifest
            .extends
            .as_deref()
            .map(|value| resolve_extends(selected_manifest, value, manifest_source))
            .transpose()?;
        let paths = target_paths(roots, project_id, target, selected_manifest)?;
        let effective_lock = paths.select_lock()?;

        Ok(Self {
            image,
            pieces,
            extends,
            inputs,
            paths,
            effective_lock,
        })
    }
}

/// The three durable paths one target owns, computed without touching the filesystem.
///
/// Separate from [`ResolvedComposition::resolve`] because `viv config` needs exactly these and
/// nothing else: it reports where the generated flake and the lock in force live whether or not
/// they exist, and it must answer that for a manifest whose image is not installed. Folding the
/// path arithmetic into composition resolution would make "where would this be written" depend on
/// every artifact the manifest names resolving first.
///
/// # Errors
///
/// Returns [`GeneratedFlakeError`] only for the impossible tree shape of a directory-form manifest
/// with no parent directory.
pub fn target_paths(
    roots: &XdgRoots,
    project_id: &str,
    target: &str,
    selected_manifest: &ResolvedArtifact,
) -> Result<GeneratedFlakePaths, GeneratedFlakeError> {
    let override_lock = if selected_manifest.form == ArtifactForm::Directory {
        Some(
            selected_manifest
                .path
                .parent()
                .ok_or_else(|| {
                    GeneratedFlakeError::plain(
                        GeneratedFlakeErrorKind::Internal,
                        Namespace::Internal,
                        "manifest-parent",
                        Locus::File(selected_manifest.path.clone()),
                        "resolved directory manifest has no parent",
                        "the resolver returned an impossible directory form",
                    )
                })?
                .join(LOCK_FILE),
        )
    } else {
        None
    };
    Ok(GeneratedFlakePaths {
        directory: roots.cache.join("flakes").join(project_id).join(target),
        owned_lock: roots
            .data
            .join("projects")
            .join(project_id)
            .join(target)
            .join(LOCK_FILE),
        override_lock,
    })
}

impl GeneratedFlakePaths {
    /// Chooses the lock in force: a present team override, else the tool-owned pin.
    ///
    /// # Errors
    ///
    /// Returns [`GeneratedFlakeError`] when a candidate cannot be inspected.
    pub fn select_lock(&self) -> Result<EffectiveLock, GeneratedFlakeError> {
        if let Some(path) = &self.override_lock
            && probe_lock(path)?
        {
            return Ok(EffectiveLock::Override { path: path.clone() });
        }
        if probe_lock(&self.owned_lock)? {
            return Ok(EffectiveLock::OwnedExisting {
                path: self.owned_lock.clone(),
            });
        }
        Ok(EffectiveLock::OwnedMissing {
            path: self.owned_lock.clone(),
        })
    }
}

impl GeneratedFlakePlan {
    /// Builds a deterministic tree plan without writing it.
    ///
    /// # Errors
    ///
    /// Returns [`GeneratedFlakeError`] if a planned destination is not a safe relative path.
    pub fn build(
        roots: &XdgRoots,
        selected_manifest: &ResolvedArtifact,
        manifest_source: &str,
        manifest: &Manifest,
        composition: &ResolvedComposition,
    ) -> Result<Self, GeneratedFlakeError> {
        let mut entries = vec![
            GeneratedEntry::RenderedFile {
                destination: PathBuf::from(FLAKE_FILE),
                bytes: render_flake(composition).into_bytes(),
            },
            GeneratedEntry::RenderedFile {
                destination: PathBuf::from(MANIFEST_MODULE_FILE),
                bytes: render_manifest_module(manifest).into_bytes(),
            },
            GeneratedEntry::RenderedFile {
                destination: PathBuf::from(MANIFEST_SOURCE_FILE),
                bytes: manifest_source.as_bytes().to_vec(),
            },
            GeneratedEntry::CopiedTree {
                source: roots.config.join("images"),
                destination: PathBuf::from("images"),
            },
            GeneratedEntry::CopiedTree {
                source: roots.config.join("pieces"),
                destination: PathBuf::from("pieces"),
            },
        ];
        if let Some(extends) = &composition.extends {
            entries.push(GeneratedEntry::CopiedTree {
                source: extends.directory.clone(),
                destination: PathBuf::from("manifests").join(&extends.manifest_name),
            });
        }
        if matches!(
            composition.effective_lock,
            EffectiveLock::Override { .. } | EffectiveLock::OwnedExisting { .. }
        ) {
            entries.push(GeneratedEntry::CopiedFile {
                source: composition.effective_lock.path().to_path_buf(),
                destination: PathBuf::from(LOCK_FILE),
            });
        }
        for entry in &entries {
            validate_relative(entry.destination())?;
        }
        let _ = selected_manifest;
        Ok(Self {
            directory: composition.paths.directory.clone(),
            entries,
            effective_lock: composition.effective_lock.clone(),
        })
    }
}

fn resolve_layer(
    config_root: &Path,
    kind: ArtifactKind,
    name: &str,
) -> Result<ResolvedArtifact, GeneratedFlakeError> {
    resolve_artifact(config_root, kind, name).map_err(|error| match error {
        super::ResolutionError::InspectArtifact { path, source } => GeneratedFlakeError::io(
            Namespace::Manifest,
            "artifact-resolution",
            path.clone(),
            format!("could not inspect artifact candidate `{}`", path.display()),
            source,
        ),
        error => GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Config,
            Namespace::Manifest,
            "artifact-resolution",
            Locus::File(config_root.to_path_buf()),
            error.to_string(),
            "the named composition layer could not be resolved",
        ),
    })
}

fn decode_inputs(bytes: Vec<u8>, path: &Path) -> Result<String, GeneratedFlakeError> {
    String::from_utf8(bytes).map_err(|_| {
        GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Config,
            Namespace::Manifest,
            "syntax",
            Locus::File(path.to_path_buf()),
            format!("`{}` is not valid TOML", path.display()),
            "expected UTF-8 text",
        )
    })
}

/// Carries the whole authored diagnostic across the type boundary.
///
/// The `accepted here:` slot is conditional but load-bearing: an unknown `inputs.toml` key is a
/// compatibility message, so dropping the accepted set here would silently truncate it.
fn input_failure(error: &super::InputError) -> GeneratedFlakeError {
    GeneratedFlakeError::plain(
        GeneratedFlakeErrorKind::Config,
        Namespace::Manifest,
        error.condition(),
        error.locus(),
        error.to_string(),
        error.why(),
    )
    .with_accepted(error.accepted())
}

fn resolve_extends(
    selected_manifest: &ResolvedArtifact,
    value: &str,
    manifest_source: &str,
) -> Result<ResolvedExtends, GeneratedFlakeError> {
    let directory = selected_manifest.path.parent().ok_or_else(|| {
        invalid_extends(
            selected_manifest,
            manifest_source,
            "the selected manifest has no containing directory",
        )
    })?;
    let canonical_directory = fs::canonicalize(directory).map_err(|source| {
        extends_io_or_invalid(selected_manifest, manifest_source, directory, source)
    })?;
    let joined = directory.join(value);
    let canonical_target = fs::canonicalize(&joined).map_err(|source| {
        extends_io_or_invalid(selected_manifest, manifest_source, &joined, source)
    })?;
    let is_nix = Path::new(value)
        .extension()
        .is_some_and(|extension| extension == "nix");
    if !is_nix || !canonical_target.starts_with(&canonical_directory) || !canonical_target.is_file()
    {
        return Err(invalid_extends(
            selected_manifest,
            manifest_source,
            "expected a regular `.nix` file that remains inside the manifest directory",
        ));
    }
    Ok(ResolvedExtends {
        directory: directory.to_path_buf(),
        relative_target: PathBuf::from(value),
        manifest_name: selected_manifest.name.clone(),
    })
}

fn extends_io_or_invalid(
    selected_manifest: &ResolvedArtifact,
    manifest_source: &str,
    path: &Path,
    source: std::io::Error,
) -> GeneratedFlakeError {
    if source.kind() == std::io::ErrorKind::PermissionDenied {
        GeneratedFlakeError::io(
            Namespace::Manifest,
            "extends-inspect",
            path.to_path_buf(),
            format!("could not inspect extends target `{}`", path.display()),
            source,
        )
    } else {
        invalid_extends(
            selected_manifest,
            manifest_source,
            "the extends target does not exist",
        )
    }
}

fn invalid_extends(
    selected_manifest: &ResolvedArtifact,
    manifest_source: &str,
    why: &str,
) -> GeneratedFlakeError {
    let locus = manifest_source.find("extends").map_or_else(
        || Locus::File(selected_manifest.path.clone()),
        |offset| Locus::in_source(&selected_manifest.path, manifest_source, offset),
    );
    GeneratedFlakeError::plain(
        GeneratedFlakeErrorKind::Config,
        Namespace::Manifest,
        "invalid-value",
        locus,
        format!(
            "invalid value for `extends` in `{}`",
            selected_manifest.name
        ),
        why,
    )
}

fn probe_lock(path: &Path) -> Result<bool, GeneratedFlakeError> {
    path.try_exists().map_err(|source| {
        GeneratedFlakeError::io(
            Namespace::Lock,
            "inspect",
            path.to_path_buf(),
            format!("could not inspect lock candidate `{}`", path.display()),
            source,
        )
    })
}

fn validate_component(component: &str, label: &'static str) -> Result<(), GeneratedFlakeError> {
    let path = Path::new(component);
    if component.is_empty()
        || path.is_absolute()
        || path.components().count() != 1
        || matches!(
            path.components().next(),
            Some(Component::CurDir | Component::ParentDir)
        )
        || component.contains(['/', '\\'])
    {
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Config,
            Namespace::Manifest,
            "invalid-value",
            Locus::Named(label),
            format!("invalid {label} `{component}`"),
            "expected one non-empty relative path component",
        ));
    }
    Ok(())
}

fn validate_relative(path: &Path) -> Result<(), GeneratedFlakeError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(GeneratedFlakeError::plain(
            GeneratedFlakeErrorKind::Internal,
            Namespace::Internal,
            "generated-path",
            Locus::Named("generated flake plan"),
            format!("unsafe generated destination `{}`", path.display()),
            "a generated destination must remain beneath its tree",
        ));
    }
    Ok(())
}

#[allow(clippy::format_push_string)]
fn render_flake(composition: &ResolvedComposition) -> String {
    let mut text = String::from(concat!(
        "{\n",
        "  description = \"vivarium generated project flake\";\n",
        "  inputs = {\n",
        "    nixpkgs.url = \"github:NixOS/nixpkgs/nixos-unstable\";\n",
        "    microvm.url = \"github:astro/microvm.nix\";\n",
        "    microvm.inputs.nixpkgs.follows = \"nixpkgs\";\n",
    ));
    for (name, input) in &composition.inputs {
        text.push_str(&format!(
            "    {name} = {{ url = \"{}\";{} }};\n",
            nix_string(&input.url),
            if input.flake { "" } else { " flake = false;" }
        ));
    }
    text.push_str("  };\n  outputs = inputs@{ nixpkgs, microvm, ... }:\n    let\n");
    text.push_str(concat!(
        "      vivariumInputs = builtins.removeAttrs inputs ",
        "[ \"self\" \"nixpkgs\" \"microvm\" ];\n",
    ));
    text.push_str("      modules = [\n");
    text.push_str(&format!("        {}\n", nix_import(&composition.image)));
    for piece in &composition.pieces {
        text.push_str(&format!("        {}\n", nix_import(piece)));
    }
    if let Some(extends) = &composition.extends {
        text.push_str(&format!(
            "        ./manifests/{}/{}\n",
            extends.manifest_name,
            extends.relative_target.display()
        ));
    }
    text.push_str("        ./manifest-leaf.nix\n        microvm.nixosModules.microvm\n      ];\n");
    text.push_str(concat!(
        "      build = system: nixpkgs.lib.nixosSystem { inherit system; ",
        "specialArgs = { inherit vivariumInputs; }; inherit modules; };\n",
    ));
    text.push_str(concat!(
        "    in { nixosConfigurations = { ",
        "x86_64-linux = build \"x86_64-linux\"; ",
        "aarch64-linux = build \"aarch64-linux\"; }; };\n}\n",
    ));
    text
}

fn nix_import(artifact: &ResolvedArtifact) -> String {
    match artifact.form {
        ArtifactForm::Flat => format!("./{}/{}.nix", artifact.kind.library(), artifact.name),
        ArtifactForm::Directory => {
            format!(
                "./{}/{}/default.nix",
                artifact.kind.library(),
                artifact.name
            )
        }
    }
}

#[allow(clippy::format_push_string)]
fn render_manifest_module(manifest: &Manifest) -> String {
    let mut value = String::from(concat!(
        "{ vivariumInputs, ... }: { _module.args = { inherit vivariumInputs; ",
        "vivariumManifest = {\n",
    ));
    value.push_str(&format!("  image = \"{}\";\n", nix_string(&manifest.image)));
    render_strings(&mut value, "pieces", &manifest.pieces);
    if let Some(extends) = &manifest.extends {
        value.push_str(&format!("  extends = \"{}\";\n", nix_string(extends)));
    }
    if let Some(resources) = manifest.resources {
        value.push_str("  resources = {");
        render_optional_u32(&mut value, "mem_mib", resources.mem_mib);
        render_optional_u32(&mut value, "vcpu", resources.vcpu);
        value.push_str(" };\n");
    }
    if let Some(egress) = &manifest.egress {
        value.push_str("  egress = {");
        if let Some(mode) = egress.mode {
            value.push_str(&format!(
                " mode = \"{}\";",
                match mode {
                    super::EgressMode::Open => "open",
                    super::EgressMode::Allowlist => "allowlist",
                }
            ));
        }
        value.push_str(" allow = [");
        for allowed in &egress.allow {
            value.push_str(&format!(" \"{}\"", nix_string(allowed)));
        }
        value.push_str(" ]; };\n");
    }
    value.push_str("  env = {");
    for (name, content) in &manifest.env {
        value.push_str(&format!(
            " \"{}\" = \"{}\";",
            nix_string(name),
            nix_string(content)
        ));
    }
    value.push_str(" };\n  mounts = [");
    for mount in &manifest.mounts {
        value.push_str(&format!(
            " {{ source = \"{}\"; target = \"{}\"; readonly = {}; }}",
            nix_string(&mount.source),
            nix_string(&mount.target),
            mount.readonly
        ));
    }
    value.push_str(" ];\n  volumes = [");
    for volume in &manifest.volumes {
        value.push_str(&format!(
            " {{ name = \"{}\"; mount = \"{}\";",
            nix_string(&volume.name),
            nix_string(&volume.mount)
        ));
        render_optional_u32(&mut value, "size_gib", volume.size_gib);
        value.push_str(" }");
    }
    value.push_str(" ];\n");
    if let Some(volume) = &manifest.volume {
        value.push_str("  volume = {");
        render_optional_u32(&mut value, "size_gib", volume.size_gib);
        value.push_str(" persist = [");
        for path in &volume.persist {
            value.push_str(&format!(" \"{}\"", nix_string(path)));
        }
        value.push_str(" ]; };\n");
    }
    value.push_str("}; }; }\n");
    value
}

#[allow(clippy::format_push_string)]
fn render_strings(output: &mut String, name: &str, values: &[String]) {
    output.push_str(&format!("  {name} = ["));
    for value in values {
        output.push_str(&format!(" \"{}\"", nix_string(value)));
    }
    output.push_str(" ];\n");
}

#[allow(clippy::format_push_string)]
fn render_optional_u32(output: &mut String, name: &str, value: Option<u32>) {
    if let Some(value) = value {
        output.push_str(&format!(" {name} = {value};"));
    }
}

fn nix_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace("${", "\\${")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use super::{EffectiveLock, GeneratedEntry, GeneratedFlakePlan, ResolvedComposition};
    use crate::config::test_support::ScratchDirectory;
    use crate::config::{ArtifactForm, ArtifactKind, Manifest, ResolvedArtifact, XdgRoots};

    #[test]
    fn resolves_paths_order_inputs_and_missing_lock() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        fs::create_dir_all(scratch.path().join("config/images/base"))?;
        fs::write(scratch.path().join("config/images/base/default.nix"), "{}")?;
        fs::write(
            scratch.path().join("config/images/base/inputs.toml"),
            "[inputs.zed]\nurl = 'path:z'\nflake = false",
        )?;
        fs::create_dir_all(scratch.path().join("config/pieces/one"))?;
        fs::write(scratch.path().join("config/pieces/one/default.nix"), "{}")?;
        fs::write(
            scratch.path().join("config/pieces/one/inputs.toml"),
            "[inputs.alpha]\nurl = 'github:a/b'",
        )?;
        let roots = roots(scratch.path());
        let selected = selected(scratch.path(), ArtifactForm::Flat);
        fs::create_dir_all(selected.path.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(&selected.path, "image = 'base'")?;
        let manifest = Manifest {
            image: "base".to_owned(),
            pieces: vec!["one".to_owned(), "one".to_owned()],
            ..Manifest::default()
        };
        let composition = ResolvedComposition::resolve(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
        )?;
        assert_eq!(composition.pieces.len(), 2);
        assert_eq!(
            composition.inputs.keys().collect::<Vec<_>>(),
            vec!["alpha", "zed"]
        );
        assert_eq!(
            composition.paths.directory,
            scratch.path().join("cache/flakes/project/default")
        );
        assert!(matches!(
            composition.effective_lock,
            EffectiveLock::OwnedMissing { .. }
        ));
        Ok(())
    }

    #[test]
    fn plan_is_semantic_deterministic_and_excludes_manifest_library()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        fs::create_dir_all(scratch.path().join("config/images"))?;
        fs::create_dir_all(scratch.path().join("config/pieces"))?;
        fs::write(scratch.path().join("config/images/base.nix"), "{}")?;
        let selected = selected(scratch.path(), ArtifactForm::Flat);
        fs::create_dir_all(selected.path.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(&selected.path, "image = 'base'")?;
        let roots = roots(scratch.path());
        let manifest = Manifest {
            image: "base".to_owned(),
            env: std::iter::once(("TOKEN".to_owned(), "quote\" slash\\ ${HOME}\n".to_owned()))
                .collect(),
            ..Manifest::default()
        };
        let composition = ResolvedComposition::resolve(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
        )?;
        let first = GeneratedFlakePlan::build(
            &roots,
            &selected,
            "image = 'base'",
            &manifest,
            &composition,
        )?;
        let second = GeneratedFlakePlan::build(
            &roots,
            &selected,
            "image = 'base'",
            &manifest,
            &composition,
        )?;
        assert_eq!(first, second);
        assert!(first.entries.iter().any(|entry| matches!(
            entry,
            GeneratedEntry::CopiedTree { destination, .. } if destination == Path::new("images")
        )));
        assert!(!first.entries.iter().any(|entry| {
            entry.destination() == Path::new("manifests")
                || entry.destination().starts_with("manifests")
        }));
        let leaf = first.entries.iter().find_map(|entry| match entry {
            GeneratedEntry::RenderedFile { destination, bytes }
                if destination == Path::new("manifest-leaf.nix") =>
            {
                Some(bytes)
            }
            _ => None,
        });
        let text = String::from_utf8(leaf.cloned().unwrap_or_default())?;
        assert!(text.contains("\\\""));
        assert!(text.contains("\\${HOME}"));
        Ok(())
    }

    #[test]
    fn extends_is_bounded_and_override_wins() -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        fs::create_dir_all(scratch.path().join("config/images"))?;
        fs::create_dir_all(scratch.path().join("config/pieces"))?;
        fs::write(scratch.path().join("config/images/base.nix"), "{}")?;
        let selected = selected(scratch.path(), ArtifactForm::Directory);
        fs::create_dir_all(selected.path.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(&selected.path, "image = 'base'\nextends = 'extra.nix'")?;
        fs::write(selected.path.with_file_name("extra.nix"), "{}")?;
        fs::write(selected.path.with_file_name("flake.lock"), "override")?;
        let roots = roots(scratch.path());
        fs::create_dir_all(scratch.path().join("data/projects/project/default"))?;
        fs::write(
            scratch
                .path()
                .join("data/projects/project/default/flake.lock"),
            "owned",
        )?;
        let manifest = Manifest {
            image: "base".to_owned(),
            extends: Some("extra.nix".to_owned()),
            ..Manifest::default()
        };
        let composition = ResolvedComposition::resolve(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'\nextends = 'extra.nix'",
            &manifest,
        )?;
        assert!(matches!(
            composition.effective_lock,
            EffectiveLock::Override { .. }
        ));
        Ok(())
    }

    /// An `inputs.toml` that reads but does not decode or parse is the author's defect, and its
    /// unknown-key form is a compatibility message that must survive the type boundary whole.
    #[test]
    fn undecodable_inputs_are_authored_defects_and_unknown_keys_keep_accepted()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        fs::create_dir_all(scratch.path().join("config/pieces"))?;
        fs::create_dir_all(scratch.path().join("config/images/base"))?;
        let inputs = scratch.path().join("config/images/base/inputs.toml");
        fs::write(scratch.path().join("config/images/base/default.nix"), "{}")?;
        let selected = selected(scratch.path(), ArtifactForm::Flat);
        fs::create_dir_all(selected.path.parent().unwrap_or_else(|| Path::new(".")))?;
        fs::write(&selected.path, "image = 'base'")?;
        let roots = roots(scratch.path());
        let manifest = Manifest {
            image: "base".to_owned(),
            ..Manifest::default()
        };

        fs::write(&inputs, [0xff, 0xfe, 0x00])?;
        let undecodable = ResolvedComposition::resolve(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
        )
        .err()
        .ok_or("undecodable inputs unexpectedly resolved")?;
        assert_eq!(undecodable.exit_code(), crate::exit::ExitKind::Config);
        assert!(
            undecodable
                .diagnostic()
                .to_string()
                .contains("manifest.syntax")
        );

        fs::write(&inputs, "[inputs.ok]\nurl = 'x'\nrevision = 'y'\n")?;
        let unknown = ResolvedComposition::resolve(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'",
            &manifest,
        )
        .err()
        .ok_or("unknown inputs key unexpectedly resolved")?;
        let rendered = unknown.diagnostic().to_string();
        assert!(rendered.contains("manifest.unknown-key"));
        assert!(rendered.contains("accepted here: url, flake"));
        Ok(())
    }

    #[test]
    fn extends_rejects_missing_and_canonical_escape_targets()
    -> Result<(), Box<dyn std::error::Error>> {
        let scratch = fixture()?;
        fs::create_dir_all(scratch.path().join("config/images"))?;
        fs::create_dir_all(scratch.path().join("config/pieces"))?;
        fs::write(scratch.path().join("config/images/base.nix"), "{}")?;
        let selected = selected(scratch.path(), ArtifactForm::Directory);
        let manifest_directory = selected.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(manifest_directory)?;
        fs::write(&selected.path, "image = 'base'\nextends = 'extra.nix'")?;
        let roots = roots(scratch.path());
        let manifest = Manifest {
            image: "base".to_owned(),
            extends: Some("extra.nix".to_owned()),
            ..Manifest::default()
        };

        let missing = ResolvedComposition::resolve(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'\nextends = 'extra.nix'",
            &manifest,
        )
        .err()
        .ok_or("missing extends target unexpectedly resolved")?;
        assert_eq!(missing.exit_code(), crate::exit::ExitKind::Config);

        let outside = scratch.path().join("config/manifests/outside.nix");
        fs::write(&outside, "{}")?;
        symlink("../outside.nix", manifest_directory.join("extra.nix"))?;
        let escaping = ResolvedComposition::resolve(
            &roots,
            "project",
            "default",
            &selected,
            "image = 'base'\nextends = 'extra.nix'",
            &manifest,
        )
        .err()
        .ok_or("escaping extends target unexpectedly resolved")?;
        assert_eq!(escaping.exit_code(), crate::exit::ExitKind::Config);
        assert!(
            escaping
                .diagnostic()
                .to_string()
                .contains("manifest.invalid-value")
        );
        Ok(())
    }

    fn fixture() -> Result<ScratchDirectory, Box<dyn std::error::Error>> {
        let scratch = ScratchDirectory::new()?;
        fs::create_dir_all(scratch.path().join("config"))?;
        Ok(scratch)
    }

    fn roots(base: &Path) -> XdgRoots {
        XdgRoots {
            config: base.join("config"),
            data: base.join("data"),
            state: base.join("state"),
            cache: base.join("cache"),
        }
    }

    fn selected(base: &Path, form: ArtifactForm) -> ResolvedArtifact {
        let path = match form {
            ArtifactForm::Flat => base.join("config/manifests/demo.toml"),
            ArtifactForm::Directory => base.join("config/manifests/demo/default.toml"),
        };
        ResolvedArtifact {
            kind: ArtifactKind::Manifest,
            name: "demo".to_owned(),
            form,
            path,
        }
    }
}
