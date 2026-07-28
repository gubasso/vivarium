# ADR-0037: Volume disk format and reclamation

## Context and Problem Statement

ADR-0019 fixed the volume _model_ — a default home volume plus named volumes, each one host-side disk image under the state root — but left the image _format_, how big it is, when it is created, and whether freed space ever comes back. Left unanswered, the natural implementation preallocates a fixed image per volume, which turns "run five projects" into a static disk bill and contradicts the ceiling model ADR-0035 established for memory.

## Considered Options

- Preallocated fixed-size raw images.
- Copy-on-write images with snapshot support.
- **Sparse raw images with a declared virtual size and discard-based reclamation.**

## Decision Outcome

Chosen option: **sparse raw with discard-based reclamation** — disk behaves exactly like memory, so one mental model covers both.

- **Format is raw, provisioned sparsely.** A copy-on-write format buys snapshots and backing chains, none of which vivarium promises; it costs a metadata layer on every write and makes "how much is this really using" a tool call instead of a file stat.
- **Declared size is a virtual ceiling.** An undeclared volume takes the default in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md). Allocation follows use.
- **Creation is lazy**: the image is materialized on the first `viv start` that needs it, not at declaration, and the filesystem is created with lazy initialization so a large virtual size costs almost nothing.
- **Reclamation is periodic, not continuous.** Guests do not mount with continuous discard — it puts the cost on every delete. A scheduled in-guest trim, plus `viv volume trim` on demand, punches the freed ranges back out of the host image.
- **Volumes grow only.** A ceiling may be raised between boots; it is never lowered in place.

## Consequences

- Good: a generous default costs nothing until used; `volume list` can show allocated against virtual.
- Bad: a sparse image can meet host exhaustion as a guest I/O error, so free space on the state filesystem must be probed.
- Bad: no snapshots, and reclamation lags until a trim runs.

## Status

Accepted

Amends [`ADR-0019-volume-model.md`](./ADR-0019-volume-model.md), which keeps ownership of the volume model itself. Applied in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) and [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).
