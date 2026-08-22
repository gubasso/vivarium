//! The manifest grammar and its parse stage.
//!
//! The key table this mirrors is `docs/reference/spec/03-artifact-model.md`, which ADR-0057 makes
//! the complete authoring surface: nothing outside it is accepted, and the accepted set at each
//! position is what an unknown-key failure has to name. Those sets are therefore held here as
//! constants rather than recovered from a deserializer's message, because ADR-0075 freezes that
//! message as a permanent API and a serde reword would otherwise change it.
//!
//! The document is walked as a spanned tree rather than deserialized into structs. That costs a
//! hand-written descent and buys the two things the failure contract needs and serde cannot give
//! together: every position comes from the parser, and every id, accepted set, and domain rule is
//! chosen by this module.
//!
//! Everything decidable from the manifest text alone fails here, at `78`. Everything that needs the
//! merged layers waits for evaluation, at `65`. That line is ADR-0057's, and it exists so that one
//! defect never carries two codes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::ArtifactForm;
use super::artifact::valid_artifact_name;
use super::error::{ManifestError, ManifestErrorKind};

/// The accepted keys at the manifest root, in the order spec/03's table lists them.
const ROOT_KEYS: &[&str] = &[
    "image",
    "pieces",
    "extends",
    "resources",
    "egress",
    "env",
    "workspaces",
    "mounts",
    "volumes",
    "volume",
];
const RESOURCE_KEYS: &[&str] = &["mem_mib", "vcpu"];
const EGRESS_KEYS: &[&str] = &["mode", "allow"];
const MOUNT_KEYS: &[&str] = &["source", "target", "readonly"];
const WORKSPACE_KEYS: &[&str] = &["source"];
const VOLUME_KEYS: &[&str] = &["name", "mount", "size_gib"];
const DEFAULT_VOLUME_KEYS: &[&str] = &["size_gib", "persist"];

/// The accepted values of `egress.mode`.
const EGRESS_MODES: &[&str] = &["open", "allowlist"];

/// The volume names vivarium reserves for the two volumes that exist without being declared.
///
/// `default` is the home volume and `store` backs the guest store's writable layer (spec/06). Both
/// are reserved for the same reason and not only the first: each is one image under the project's
/// state at `volumes/<name>.img`, so a manifest declaring either would name a file that already
/// belongs to something else — and the guest, which resolves volumes by label, would be the place
/// that found out.
const RESERVED_VOLUME_NAMES: &[&str] = &["default", "store"];

/// The smallest memory ceiling spec/03 admits, in MiB.
const MINIMUM_MEM_MIB: i64 = 256;

/// Which file a parse is reading, and in which of the two artifact forms.
///
/// Carried as a parameter rather than discovered here because the grammar module reads no
/// filesystem: the form is what `resolve_artifact` already decided, and `extends` is legal only in
/// the directory form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestOrigin {
    /// The bare kebab-case manifest name selected from the library.
    pub name: String,
    /// The file the text came from, used for the `-->` slot.
    pub path: PathBuf,
    /// Which of the two forms resolution found.
    pub form: ArtifactForm,
}

/// One manifest, validated against every rule the text alone can decide.
///
/// An absent optional table stays `None` rather than collapsing to a default, because spec/03 is
/// explicit that an absent table is never an empty one: an undeclared ceiling resolves from the
/// host at launch, and zero would be a different and wrong answer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Manifest {
    /// The one image this manifest names.
    pub image: String,
    /// The ordered pieces layered onto it.
    pub pieces: Vec<String>,
    /// The escape-hatch module, legal only in the directory form.
    pub extends: Option<String>,
    /// Declared ceilings, absent when the host resolves them.
    pub resources: Option<Resources>,
    /// Declared egress policy, absent when the policy default applies.
    pub egress: Option<Egress>,
    /// Guest environment, empty when undeclared.
    pub env: BTreeMap<String, String>,
    /// Host paths mirrored into the guest.
    pub mounts: Vec<Mount>,
    /// Host trees owned by and mirrored into this sandbox.
    pub workspaces: Vec<Workspace>,
    /// Declared volumes beyond the home volume.
    pub volumes: Vec<Volume>,
    /// Configuration of the home volume, which exists without declaration.
    pub volume: Option<DefaultVolume>,
}

/// Ceilings, not reservations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Resources {
    /// Memory ceiling in MiB.
    pub mem_mib: Option<u32>,
    /// Virtual CPU ceiling.
    pub vcpu: Option<u32>,
}

/// The egress policy knob, compiled to the `sandbox.egress` module options.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Egress {
    /// `open` or `allowlist`.
    pub mode: Option<EgressMode>,
    /// Destination patterns, each already validated against spec/05's entry grammar.
    pub allow: Vec<String>,
}

/// The two egress modes spec/05 defines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EgressMode {
    /// Outbound traffic is unrestricted.
    Open,
    /// Only the declared destinations are reachable.
    Allowlist,
}

/// One host path mirrored into the guest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mount {
    /// Host path. `${VAR}` stays unexpanded here and is resolved at launch.
    pub source: String,
    /// Guest path. `~` is the guest home.
    pub target: String,
    /// Whether the guest sees it read-only.
    pub readonly: bool,
}

/// One host tree owned by this sandbox.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workspace {
    /// Host path. `${VAR}` stays unexpanded here and is resolved at launch.
    pub source: String,
}

/// One declared volume.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Volume {
    /// Kebab-case name, never `default`.
    pub name: String,
    /// Absolute guest mountpoint.
    pub mount: String,
    /// Virtual ceiling in GiB, absent when the policy default applies.
    pub size_gib: Option<u32>,
}

/// Configuration of the home volume.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DefaultVolume {
    /// Virtual ceiling in GiB, absent when the policy default applies.
    pub size_gib: Option<u32>,
    /// Absolute guest paths that survive a rebuild.
    pub persist: Vec<String>,
}

/// Parses one manifest and validates every rule decidable from its own text.
///
/// Takes the text rather than a path so the whole grammar is exercisable without a filesystem, the
/// same seam `resolve_xdg_roots` established for the environment.
///
/// # Errors
///
/// Returns [`ManifestError`] for malformed TOML, an unknown key, a missing required key, a wrong
/// type, a value outside its domain, or `extends` in the flat form. Every one of them is `78`.
pub fn parse_manifest(source: &str, origin: &ManifestOrigin) -> Result<Manifest, ManifestError> {
    let reader = Reader { source, origin };
    let document = DeTable::parse(source).map_err(|error| {
        reader.error(
            error.span().map(|span| span.start),
            ManifestErrorKind::Syntax {
                detail: error.message().to_owned(),
            },
        )
    })?;
    let root = document.get_ref();

    // The unknown-key sweep runs first and over the whole document, so the message that carries
    // vivarium's entire compatibility signal reports the earliest offending key rather than
    // whichever one the typed walk happens to reach first. Entries are stored lexicographically,
    // so document order has to be recovered from the spans.
    if let Some(error) = reader.first_unknown_key(root) {
        return Err(error);
    }

    reader.manifest(root)
}

/// The parse in progress, holding what every failure needs to name itself.
struct Reader<'a> {
    source: &'a str,
    origin: &'a ManifestOrigin,
}

impl Reader<'_> {
    fn error(&self, offset: Option<usize>, kind: ManifestErrorKind) -> ManifestError {
        ManifestError::new(
            kind,
            &self.origin.name,
            &self.origin.path,
            self.source,
            offset,
        )
    }

    fn manifest(&self, root: &DeTable<'_>) -> Result<Manifest, ManifestError> {
        let image = match entry(root, "image") {
            Some(value) => self.name(value, "image")?,
            None => {
                return Err(self.error(None, ManifestErrorKind::MissingKey { key: "image" }));
            }
        };

        let mut pieces = Vec::new();
        if let Some(value) = entry(root, "pieces") {
            for element in self.array(value, "pieces")? {
                pieces.push(self.name(element, "pieces")?);
            }
        }

        let extends = match entry(root, "extends") {
            Some(value) => Some(self.extends(value)?),
            None => None,
        };

        Ok(Manifest {
            image,
            pieces,
            extends,
            resources: self.resources(root)?,
            egress: self.egress(root)?,
            env: self.env(root)?,
            workspaces: self.workspaces(root)?,
            mounts: self.mounts(root)?,
            volumes: self.volumes(root)?,
            volume: self.default_volume(root)?,
        })
    }

    fn extends(&self, value: &Spanned<DeValue<'_>>) -> Result<String, ManifestError> {
        // The flat form is one file, so a relative module beside it cannot exist. spec/03 puts this
        // on the parse side of the boundary because the form is known before anything is merged.
        if self.origin.form == ArtifactForm::Flat {
            return Err(self.error(
                Some(value.span().start),
                ManifestErrorKind::ExtendsRequiresDirectoryForm,
            ));
        }
        let path = self.string(value, "extends")?;
        // Exactly `.nix`, case included: the extension is matched against a real filename on a
        // case-sensitive filesystem, so accepting `.NIX` here would only defer the failure.
        let relative = std::path::Path::new(&path);
        if relative.is_absolute() || relative.extension() != Some(std::ffi::OsStr::new("nix")) {
            return Err(self.invalid(value, "extends", "a relative path to a `.nix` module", &[]));
        }
        Ok(path)
    }

    fn resources(&self, root: &DeTable<'_>) -> Result<Option<Resources>, ManifestError> {
        let Some(value) = entry(root, "resources") else {
            return Ok(None);
        };
        let table = self.table(value, "resources")?;
        Ok(Some(Resources {
            mem_mib: self.bounded(table, "resources.mem_mib", "mem_mib", MINIMUM_MEM_MIB)?,
            vcpu: self.bounded(table, "resources.vcpu", "vcpu", 1)?,
        }))
    }

    fn egress(&self, root: &DeTable<'_>) -> Result<Option<Egress>, ManifestError> {
        let Some(value) = entry(root, "egress") else {
            return Ok(None);
        };
        let table = self.table(value, "egress")?;

        let mode = match entry(table, "mode") {
            Some(entry) => match self.string(entry, "egress.mode")?.as_str() {
                "open" => Some(EgressMode::Open),
                "allowlist" => Some(EgressMode::Allowlist),
                _ => {
                    return Err(self.invalid(
                        entry,
                        "egress.mode",
                        "one of the two egress modes",
                        EGRESS_MODES,
                    ));
                }
            },
            None => None,
        };

        let mut allow = Vec::new();
        if let Some(entry) = entry(table, "allow") {
            for element in self.array(entry, "egress.allow")? {
                let pattern = self.string(element, "egress.allow")?;
                if crate::net::allowlist::AllowEntry::parse(&pattern).is_err() {
                    return Err(self.invalid(
                        element,
                        "egress.allow",
                        "a name, wildcard name, address, or CIDR block",
                        &[],
                    ));
                }
                allow.push(pattern);
            }
        }

        Ok(Some(Egress { mode, allow }))
    }

    fn env(&self, root: &DeTable<'_>) -> Result<BTreeMap<String, String>, ManifestError> {
        let mut env = BTreeMap::new();
        let Some(value) = entry(root, "env") else {
            return Ok(env);
        };
        for (key, element) in self.table(value, "env")? {
            let name = key.get_ref().as_ref();
            // Every name is accepted here, so a malformed one is an out-of-domain value rather than
            // an unknown key — which is why the sweep above does not descend into this table.
            if !valid_environment_name(name) {
                return Err(self.error(
                    Some(key.span().start),
                    ManifestErrorKind::InvalidValue {
                        key: format!("env.{name}"),
                        expected: "a name matching `^[A-Za-z_][A-Za-z0-9_]*$`",
                        accepted: &[],
                    },
                ));
            }
            env.insert(name.to_owned(), self.string(element, "env")?);
        }
        Ok(env)
    }

    fn mounts(&self, root: &DeTable<'_>) -> Result<Vec<Mount>, ManifestError> {
        let Some(value) = entry(root, "mounts") else {
            return Ok(Vec::new());
        };
        let mut mounts = Vec::new();
        for element in self.array(value, "mounts")? {
            let table = self.table(element, "mounts")?;
            mounts.push(Mount {
                source: self.required_path(table, element, "mounts.source", "source")?,
                target: self.required_path(table, element, "mounts.target", "target")?,
                readonly: match entry(table, "readonly") {
                    Some(flag) => self.boolean(flag, "mounts.readonly")?,
                    None => false,
                },
            });
        }
        Ok(mounts)
    }

    fn workspaces(&self, root: &DeTable<'_>) -> Result<Vec<Workspace>, ManifestError> {
        let Some(value) = entry(root, "workspaces") else {
            return Ok(Vec::new());
        };
        let mut workspaces = Vec::new();
        for element in self.array(value, "workspaces")? {
            let table = self.table(element, "workspaces")?;
            workspaces.push(Workspace {
                source: self.required_path(table, element, "workspaces.source", "source")?,
            });
        }
        Ok(workspaces)
    }

    fn volumes(&self, root: &DeTable<'_>) -> Result<Vec<Volume>, ManifestError> {
        let Some(value) = entry(root, "volumes") else {
            return Ok(Vec::new());
        };
        let mut volumes = Vec::new();
        for element in self.array(value, "volumes")? {
            let table = self.table(element, "volumes")?;

            let Some(name_entry) = entry(table, "name") else {
                return Err(self.error(
                    Some(element.span().start),
                    ManifestErrorKind::MissingKey { key: "name" },
                ));
            };
            let name = self.name(name_entry, "volumes.name")?;
            if RESERVED_VOLUME_NAMES.contains(&name.as_str()) {
                return Err(self.invalid(
                    name_entry,
                    "volumes.name",
                    "a name other than the reserved home and store volumes",
                    &[],
                ));
            }

            let Some(mount_entry) = entry(table, "mount") else {
                return Err(self.error(
                    Some(element.span().start),
                    ManifestErrorKind::MissingKey { key: "mount" },
                ));
            };
            let mount = self.string(mount_entry, "volumes.mount")?;
            if !std::path::Path::new(&mount).is_absolute() {
                return Err(self.invalid(
                    mount_entry,
                    "volumes.mount",
                    "an absolute guest path",
                    &[],
                ));
            }

            volumes.push(Volume {
                name,
                mount,
                size_gib: self.bounded(table, "volumes.size_gib", "size_gib", 1)?,
            });
        }
        Ok(volumes)
    }

    fn default_volume(&self, root: &DeTable<'_>) -> Result<Option<DefaultVolume>, ManifestError> {
        let Some(value) = entry(root, "volume") else {
            return Ok(None);
        };
        let table = self.table(value, "volume")?;

        let mut persist = Vec::new();
        if let Some(entry) = entry(table, "persist") {
            for element in self.array(entry, "volume.persist")? {
                let path = self.string(element, "volume.persist")?;
                if !std::path::Path::new(&path).is_absolute() {
                    return Err(self.invalid(
                        element,
                        "volume.persist",
                        "an absolute guest path",
                        &[],
                    ));
                }
                persist.push(path);
            }
        }

        Ok(Some(DefaultVolume {
            size_gib: self.bounded(table, "volume.size_gib", "size_gib", 1)?,
            persist,
        }))
    }

    /// Reads an optional integer and rejects anything below its floor or above `u32`.
    fn bounded(
        &self,
        table: &DeTable<'_>,
        key: &'static str,
        member: &str,
        minimum: i64,
    ) -> Result<Option<u32>, ManifestError> {
        let Some(value) = entry(table, member) else {
            return Ok(None);
        };
        let number = self.integer(value, key)?;
        u32::try_from(number)
            .ok()
            .filter(|_| number >= minimum)
            .map(Some)
            .ok_or_else(|| {
                self.invalid(
                    value,
                    key,
                    if minimum == MINIMUM_MEM_MIB {
                        "an integer of at least 256"
                    } else {
                        "an integer of at least 1"
                    },
                    &[],
                )
            })
    }

    fn required_path(
        &self,
        table: &DeTable<'_>,
        parent: &Spanned<DeValue<'_>>,
        key: &'static str,
        member: &'static str,
    ) -> Result<String, ManifestError> {
        let Some(value) = entry(table, member) else {
            return Err(self.error(
                Some(parent.span().start),
                ManifestErrorKind::MissingKey { key: member },
            ));
        };
        let path = self.string(value, key)?;
        if path.is_empty() {
            return Err(self.invalid(value, key, "a non-empty path", &[]));
        }
        Ok(path)
    }

    fn name(
        &self,
        value: &Spanned<DeValue<'_>>,
        key: &'static str,
    ) -> Result<String, ManifestError> {
        let name = self.string(value, key)?;
        if valid_artifact_name(&name) {
            Ok(name)
        } else {
            Err(self.invalid(value, key, "a kebab-case name", &[]))
        }
    }

    fn string(&self, value: &Spanned<DeValue<'_>>, key: &str) -> Result<String, ManifestError> {
        match value.get_ref() {
            DeValue::String(text) => Ok(text.as_ref().to_owned()),
            other => Err(self.wrong_type(value, key, "a string", other)),
        }
    }

    fn boolean(&self, value: &Spanned<DeValue<'_>>, key: &str) -> Result<bool, ManifestError> {
        match value.get_ref() {
            DeValue::Boolean(flag) => Ok(*flag),
            other => Err(self.wrong_type(value, key, "a boolean", other)),
        }
    }

    fn integer(&self, value: &Spanned<DeValue<'_>>, key: &str) -> Result<i64, ManifestError> {
        match value.get_ref() {
            DeValue::Integer(number) => i64::from_str_radix(number.as_str(), number.radix())
                .map_err(|_| self.invalid(value, key, "an integer that fits in 64 bits", &[])),
            other => Err(self.wrong_type(value, key, "an integer", other)),
        }
    }

    fn array<'v>(
        &self,
        value: &'v Spanned<DeValue<'_>>,
        key: &str,
    ) -> Result<&'v [Spanned<DeValue<'v>>], ManifestError> {
        match value.get_ref() {
            DeValue::Array(elements) => Ok(elements),
            other => Err(self.wrong_type(value, key, "an array", other)),
        }
    }

    fn table<'v>(
        &self,
        value: &'v Spanned<DeValue<'_>>,
        key: &str,
    ) -> Result<&'v DeTable<'v>, ManifestError> {
        match value.get_ref() {
            DeValue::Table(table) => Ok(table),
            other => Err(self.wrong_type(value, key, "a table", other)),
        }
    }

    fn wrong_type(
        &self,
        value: &Spanned<DeValue<'_>>,
        key: &str,
        expected: &'static str,
        found: &DeValue<'_>,
    ) -> ManifestError {
        self.error(
            Some(value.span().start),
            ManifestErrorKind::WrongType {
                key: key.to_owned(),
                expected,
                found: type_name(found),
            },
        )
    }

    fn invalid(
        &self,
        value: &Spanned<DeValue<'_>>,
        key: &str,
        expected: &'static str,
        accepted: &'static [&'static str],
    ) -> ManifestError {
        self.error(
            Some(value.span().start),
            ManifestErrorKind::InvalidValue {
                key: key.to_owned(),
                expected,
                accepted,
            },
        )
    }

    /// Finds the earliest unknown key anywhere in the document.
    ///
    /// Descends only through keys the grammar knows, so a table that is itself unknown is reported
    /// once by its own name instead of once per key inside it.
    fn first_unknown_key(&self, root: &DeTable<'_>) -> Option<ManifestError> {
        let mut found: Vec<(usize, String, &'static [&'static str])> = Vec::new();
        collect_unknown_keys(root, ROOT_KEYS, &mut found);
        found.sort_by_key(|(offset, ..)| *offset);
        found.into_iter().next().map(|(offset, key, accepted)| {
            self.error(
                Some(offset),
                ManifestErrorKind::UnknownKey { key, accepted },
            )
        })
    }
}

/// Walks one table and everything the grammar knows how to descend into.
fn collect_unknown_keys(
    table: &DeTable<'_>,
    accepted: &'static [&'static str],
    found: &mut Vec<(usize, String, &'static [&'static str])>,
) {
    for (key, value) in table {
        let name = key.get_ref().as_ref();
        if !accepted.contains(&name) {
            found.push((key.span().start, name.to_owned(), accepted));
            continue;
        }
        // `env` accepts every name it is given, so it has no accepted set to check against; its
        // names are validated as values instead.
        let nested = match name {
            "resources" => Some(RESOURCE_KEYS),
            "egress" => Some(EGRESS_KEYS),
            "workspaces" => Some(WORKSPACE_KEYS),
            "mounts" => Some(MOUNT_KEYS),
            "volumes" => Some(VOLUME_KEYS),
            "volume" => Some(DEFAULT_VOLUME_KEYS),
            _ => None,
        };
        if let Some(nested) = nested {
            match value.get_ref() {
                DeValue::Table(inner) => collect_unknown_keys(inner, nested, found),
                DeValue::Array(elements) => {
                    for element in elements {
                        if let DeValue::Table(inner) = element.get_ref() {
                            collect_unknown_keys(inner, nested, found);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Looks a key up in a spanned table, whose keys are themselves spanned.
fn entry<'t>(table: &'t DeTable<'_>, key: &str) -> Option<&'t Spanned<DeValue<'t>>> {
    table
        .iter()
        .find(|(name, _)| name.get_ref().as_ref() == key)
        .map(|(_, value)| value)
}

/// The TOML type of a value, for the `why` slot of a wrong-type failure.
const fn type_name(value: &DeValue<'_>) -> &'static str {
    match value {
        DeValue::String(_) => "a string",
        DeValue::Integer(_) => "an integer",
        DeValue::Float(_) => "a float",
        DeValue::Boolean(_) => "a boolean",
        DeValue::Datetime(_) => "a datetime",
        DeValue::Array(_) => "an array",
        DeValue::Table(_) => "a table",
    }
}

/// The environment-name grammar spec/03 fixes: `^[A-Za-z_][A-Za-z0-9_]*$`.
fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{ArtifactForm, EgressMode, Manifest, ManifestOrigin, Resources, parse_manifest};
    use crate::exit::ExitKind;

    fn origin(form: ArtifactForm) -> ManifestOrigin {
        ManifestOrigin {
            name: "rust-web".to_owned(),
            path: "manifests/rust-web.toml".into(),
            form,
        }
    }

    fn parse(source: &str) -> Result<Manifest, crate::config::ManifestError> {
        parse_manifest(source, &origin(ArtifactForm::Flat))
    }

    /// Pins the whole spec/03 example as accepted, key for key.
    #[test]
    fn the_specified_example_manifest_parses() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = parse(
            r#"
image  = "rust"
pieces = [ "git", "ssh-agent", "direnv" ]

[resources]
mem_mib = 4096
vcpu    = 4

[egress]
mode  = "open"
allow = [ ]

[env]
RUST_BACKTRACE = "1"

[[workspaces]]
source = "${HOME}/src/rust-web"

[[mounts]]
source   = "${HOME}/.config/foo"
target   = "~/.config/foo"
readonly = true

[[volumes]]
name     = "cache"
mount    = "/var/cache/project"
size_gib = 64

[volume]
size_gib = 32
persist  = [ "/opt/state" ]
"#,
        )?;

        assert_eq!(manifest.image, "rust");
        assert_eq!(manifest.pieces, ["git", "ssh-agent", "direnv"]);
        assert_eq!(
            manifest.resources,
            Some(Resources {
                mem_mib: Some(4096),
                vcpu: Some(4),
            })
        );
        let egress = manifest.egress.ok_or("egress table was dropped")?;
        assert_eq!(egress.mode, Some(EgressMode::Open));
        assert!(egress.allow.is_empty());
        assert_eq!(
            manifest.env.get("RUST_BACKTRACE").map(String::as_str),
            Some("1")
        );
        assert_eq!(manifest.workspaces[0].source, "${HOME}/src/rust-web");
        assert_eq!(manifest.mounts.len(), 1);
        assert!(manifest.mounts[0].readonly);
        assert_eq!(manifest.volumes[0].size_gib, Some(64));
        assert_eq!(manifest.volume.and_then(|volume| volume.size_gib), Some(32));
        Ok(())
    }

    /// Pins `image` as the only required key, and every table as optional.
    #[test]
    fn image_is_the_only_required_key() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = parse("image = \"minimal\"\n")?;
        assert_eq!(manifest.image, "minimal");
        assert!(manifest.pieces.is_empty());
        assert!(manifest.resources.is_none());
        assert!(manifest.egress.is_none());
        assert!(manifest.volume.is_none());

        assert!(matches!(
            parse("pieces = [ ]\n").err().map(|error| error.kind_name()),
            Some("missing-key")
        ));
        Ok(())
    }

    #[test]
    fn workspace_rows_preserve_zero_one_and_many_in_order() -> Result<(), Box<dyn Error>> {
        assert!(parse("image = 'x'\n")?.workspaces.is_empty());
        let one = parse("image = 'x'\n[[workspaces]]\nsource = '/one'\n")?;
        assert_eq!(one.workspaces[0].source, "/one");
        let many = parse(
            "image = 'x'\n[[workspaces]]\nsource = '/one'\n[[workspaces]]\nsource = '/two'\n",
        )?;
        assert_eq!(
            many.workspaces
                .iter()
                .map(|workspace| workspace.source.as_str())
                .collect::<Vec<_>>(),
            ["/one", "/two"]
        );
        Ok(())
    }

    /// Pins the absent-is-not-empty rule: an undeclared ceiling must stay undeclared, because
    /// spec/03 resolves it from the host at launch and zero would be a different answer.
    #[test]
    fn an_absent_table_is_not_an_empty_one() -> Result<(), Box<dyn std::error::Error>> {
        assert!(parse("image = \"minimal\"\n")?.resources.is_none());
        assert_eq!(
            parse("image = \"minimal\"\n[resources]\n")?.resources,
            Some(Resources {
                mem_mib: None,
                vcpu: None,
            })
        );
        Ok(())
    }

    /// One row per key in spec/03's table. The row count is the coverage argument: every accepted
    /// value parses and every out-of-domain value is refused at parse.
    #[test]
    fn every_key_holds_its_specified_domain() {
        let accepted = [
            "image = \"rust\"",
            "image = \"a\"",
            "image = \"a-b-1\"",
            "image = \"x\"\npieces = [ \"git\" ]",
            "image = \"x\"\n[resources]\nmem_mib = 256",
            "image = \"x\"\n[resources]\nvcpu = 1",
            "image = \"x\"\n[egress]\nmode = \"open\"",
            "image = \"x\"\n[egress]\nmode = \"allowlist\"",
            "image = \"x\"\n[egress]\nallow = [ \"github.com\" ]",
            "image = \"x\"\n[egress]\nallow = [ \"*.github.com\", \"**.example.org\" ]",
            "image = \"x\"\n[egress]\nallow = [ \"192.0.2.10\", \"2001:db8::/32\" ]",
            "image = \"x\"\n[env]\n_FOO9 = \"1\"",
            "image = \"x\"\n[[workspaces]]\nsource = \"/workspace\"",
            "image = \"x\"\n[[mounts]]\nsource = \"/a\"\ntarget = \"~/a\"",
            "image = \"x\"\n[[volumes]]\nname = \"cache\"\nmount = \"/v\"\nsize_gib = 1",
            "image = \"x\"\n[volume]\npersist = [ \"/opt/a\" ]",
        ];
        for source in accepted {
            assert!(parse(source).is_ok(), "refused a valid manifest: {source}");
        }

        let refused = [
            ("image = \"Rust\"", "invalid-value"), // not kebab-case
            ("image = 1", "wrong-type"),           // wrong type
            ("image = \"x\"\npieces = \"git\"", "wrong-type"),
            ("image = \"x\"\npieces = [ \"Git\" ]", "invalid-value"),
            ("image = \"x\"\n[resources]\nmem_mib = 255", "invalid-value"),
            ("image = \"x\"\n[resources]\nmem_mib = 0", "invalid-value"),
            ("image = \"x\"\n[resources]\nvcpu = 0", "invalid-value"),
            ("image = \"x\"\n[resources]\nvcpu = \"4\"", "wrong-type"),
            ("image = \"x\"\n[egress]\nmode = \"off\"", "invalid-value"),
            (
                "image = \"x\"\n[egress]\nallow = [ \"example.com:443\" ]",
                "invalid-value",
            ),
            (
                "image = \"x\"\n[egress]\nallow = [ \"***.example.com\" ]",
                "invalid-value",
            ),
            ("image = \"x\"\n[env]\n9FOO = \"1\"", "invalid-value"),
            ("image = \"x\"\n[env]\nFOO = 1", "wrong-type"),
            ("image = \"x\"\n[[workspaces]]", "missing-key"),
            (
                "image = \"x\"\n[[workspaces]]\nsource = \"\"",
                "invalid-value",
            ),
            (
                "image = \"x\"\n[[workspaces]]\nsource = \"/a\"\ntarget = \"/b\"",
                "unknown-key",
            ),
            ("image = \"x\"\n[[mounts]]\ntarget = \"~/a\"", "missing-key"),
            (
                "image = \"x\"\n[[mounts]]\nsource = \"\"\ntarget = \"~/a\"",
                "invalid-value",
            ),
            ("image = \"x\"\n[[volumes]]\nmount = \"/v\"", "missing-key"),
            (
                "image = \"x\"\n[[volumes]]\nname = \"default\"\nmount = \"/v\"",
                "invalid-value",
            ),
            // Both reserved names, not only the home volume's. `store.img` already exists under
            // every project's state, so a manifest naming it would have the guest resolve two
            // volumes to one label and the loser would be whichever the boot happened to miss.
            (
                "image = \"x\"\n[[volumes]]\nname = \"store\"\nmount = \"/v\"",
                "invalid-value",
            ),
            (
                "image = \"x\"\n[[volumes]]\nname = \"c\"\nmount = \"relative\"",
                "invalid-value",
            ),
            (
                "image = \"x\"\n[[volumes]]\nname = \"c\"\nmount = \"/v\"\nsize_gib = 0",
                "invalid-value",
            ),
            (
                "image = \"x\"\n[volume]\npersist = [ \"relative\" ]",
                "invalid-value",
            ),
            ("image = \"x\"\nimage2 = \"y\"", "unknown-key"),
            ("image = \"x\"\n[resources]\nmemory = 1", "unknown-key"),
            ("image = \"x\" =", "syntax"),
        ];
        for (source, kind) in refused {
            let error = parse(source).err();
            assert_eq!(
                error.as_ref().map(super::ManifestError::kind_name),
                Some(kind),
                "wrong classification for: {source}"
            );
            assert_eq!(
                error.map(|error| error.exit_code()),
                Some(ExitKind::Config),
                "every parse defect is 78: {source}"
            );
        }
    }

    /// Pins the unknown-key message as the compatibility surface, with all five parts present.
    #[test]
    fn the_unknown_key_message_carries_its_five_parts() -> Result<(), Box<dyn std::error::Error>> {
        let error = parse("image = \"rust\"\npieces = [ ]\nschema_version = 2\n")
            .err()
            .ok_or("a manifest with an unknown key parsed")?;
        let rendered = error.diagnostic().to_string();

        assert_eq!(
            rendered,
            format!(
                concat!(
                    "error[manifest.unknown-key]: unknown key `schema_version` ",
                    "in manifest `rust-web`\n",
                    "  --> manifests/rust-web.toml:3:1\n",
                    "  why: not part of the manifest grammar viv {version} understands\n",
                    "  accepted here: image, pieces, extends, resources, egress, ",
                    "env, workspaces, mounts, volumes, volume\n",
                    "  hint: remove the key, or upgrade vivarium — a manifest written ",
                    "for a newer\n",
                    "        vivarium reports its new keys exactly this way",
                ),
                version = env!("CARGO_PKG_VERSION")
            )
        );
        Ok(())
    }

    /// Pins the earliest offending key as the one reported, which lexicographic storage would
    /// otherwise decide instead.
    #[test]
    fn the_earliest_unknown_key_is_the_one_reported() -> Result<(), Box<dyn std::error::Error>> {
        let error = parse("image = \"rust\"\nalpha = 1\nzulu = 2\n")
            .err()
            .ok_or("a manifest with two unknown keys parsed")?;
        assert!(error.to_string().contains("`alpha`"));

        let reversed = parse("image = \"rust\"\nzulu = 2\nalpha = 1\n")
            .err()
            .ok_or("a manifest with two unknown keys parsed")?;
        assert!(reversed.to_string().contains("`zulu`"));
        Ok(())
    }

    /// Pins an unknown table as reported once by its own name, not once per key inside it.
    #[test]
    fn an_unknown_table_is_reported_by_its_own_name() -> Result<(), Box<dyn std::error::Error>> {
        let error = parse("image = \"rust\"\n[network]\nmode = \"bridge\"\n")
            .err()
            .ok_or("a manifest with an unknown table parsed")?;
        assert!(error.to_string().contains("`network`"));
        Ok(())
    }

    /// Pins `extends` to the directory form, which spec/03 decides from the form alone.
    #[test]
    fn extends_is_refused_in_the_flat_form() -> Result<(), Box<dyn std::error::Error>> {
        let source = "image = \"rust\"\nextends = \"./custom.nix\"\n";
        assert_eq!(
            parse(source).err().map(|error| error.kind_name()),
            Some("extends-form")
        );

        let directory = parse_manifest(source, &origin(ArtifactForm::Directory))?;
        assert_eq!(directory.extends.as_deref(), Some("./custom.nix"));

        assert_eq!(
            parse_manifest(
                "image = \"rust\"\nextends = \"/abs/custom.nix\"\n",
                &origin(ArtifactForm::Directory)
            )
            .err()
            .map(|error| error.kind_name()),
            Some("invalid-value")
        );
        assert_eq!(
            parse_manifest(
                "image = \"rust\"\nextends = \"./custom.txt\"\n",
                &origin(ArtifactForm::Directory)
            )
            .err()
            .map(|error| error.kind_name()),
            Some("invalid-value")
        );
        Ok(())
    }

    /// Pins the position slot as one-based and pointing at the offending token, which is the part
    /// of the compatibility message a reader acts on.
    #[test]
    fn a_failure_points_at_its_own_token() -> Result<(), Box<dyn std::error::Error>> {
        let error = parse("image = \"rust\"\n\n[resources]\nmem_mib = 0\n")
            .err()
            .ok_or("an out-of-domain ceiling parsed")?;
        assert!(
            error
                .diagnostic()
                .to_string()
                .contains("rust-web.toml:4:11"),
            "unexpected position: {}",
            error.diagnostic()
        );
        Ok(())
    }

    /// The seam slice 004 closed: every entry meets spec/05's grammar at parse, refused rather
    /// than narrowed, so no malformed pattern survives to the enforcement path.
    #[test]
    fn egress_patterns_hold_their_grammar_at_parse() {
        let error = parse("image = \"x\"\n[egress]\nallow = [ \"not a pattern\" ]\n").err();
        assert_eq!(
            error.as_ref().map(super::ManifestError::kind_name),
            Some("invalid-value")
        );
        // A scheme is refused, not stripped to its name (the allowlist grammar's
        // refuse-not-narrow rule, surfaced through manifest validation).
        let error = parse("image = \"x\"\n[egress]\nallow = [ \"https://example.com\" ]\n").err();
        assert_eq!(
            error.as_ref().map(super::ManifestError::kind_name),
            Some("invalid-value")
        );
    }
}
