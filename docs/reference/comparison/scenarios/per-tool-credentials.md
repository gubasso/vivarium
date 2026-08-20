# Per-tool credentials

Every subject can narrow what crosses by declaring or passing less. This row asks the narrower question: whether the tool arrives already knowing which credential directory belongs to which application, so the scoping happens without the user mapping it.

1. Authenticate two different applications so each writes its own credential directory.
2. Start the sandbox for one of them, naming only the application.
3. Inside, attempt to read the other's credential path, and record the result.

## vivarium

vivarium `ceb0027`, 2026-08-18. No, by design: `[[mounts]]` is declared per manifest, so every process in the guest sees every declared mount, and narrowing means the user declaring less. Automatic scoping would require vivarium to hold a table of application names and the credential paths each one wants — a hidden mapping between a tool the user did not name and a host path they did not declare. vivarium takes the opposite position: it is application-agnostic, every path that crosses is one the user wrote down, and no command has a side effect the manifest does not show. The manifest is where a user narrows this, and it is the only place.

The position is consistent with the [config-read-only rule](../../spec/08-invariants-and-guarantees.md) and the [never-touch-user-files rule](../../spec/08-invariants-and-guarantees.md), which refuse side effects on user-owned surfaces for the same reason, but no invariant states application-agnosticism itself. Until one does, this cell is the only `†` in the set resting on a design position rather than a binding rule; the question is logged as `Q-031` in [`open-questions.md`](../../../plan/open-questions.md).

## flake-pilot

flake-pilot `main`, read 2026-08-18. No: at the firecracker boundary a credential arrives as an `--include-path` copy fixed at registration time, so the payload is frozen per registered application rather than selected per run. One registration, one baked-in set, and no per-consumer view of it once the guest is up.

## glaipnir

glaipnir `21ef389`, read 2026-08-18. Yes: `_bind_agent_mounts` holds a table mapping seven agent names to the credential directories each one owns, and emits only the volumes the selected agents need, so `run claude` never mounts the `gh` token — it is absent from the guest rather than hidden inside it. The volumes cross into the krun guest as virtiofs, so the scoping holds at the compared setup.

What it costs is the reason vivarium answers the other way. The table is seven hardcoded names, so an agent glaipnir does not know gets no scoping, and a user who wants a different mapping edits the tool rather than a file they own. The same built-in opinion is what loses glaipnir [Defined by a project file](./project-file.md#glaipnir) and [Choose the guest operating system](./guest-os.md#glaipnir).

Naming the agent is what scopes the mounts; nothing else is passed:

```bash
glaipnir run claude
#   -> claude auth login, inside the sandbox
#   -> persists to ~/.cache/glaipnir/agents-mount/.claude/
#   -> the gh token is not mounted at all
```

## podman

podman 5.x, read 2026-08-19. No: `-v` is a flat list assembled at the call site and podman has no notion of which process inside needs which mount, so everything passed is visible to everything in the guest. Launching one container per tool with a different `-v` set reproduces the effect, and that is the user doing it per invocation rather than the tool providing it.
