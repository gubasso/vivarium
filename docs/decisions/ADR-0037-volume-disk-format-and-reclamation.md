# ADR-0037: Volume disk format and reclamation

## Context and Problem Statement

ADR-0019 fixed the volume model — a default home volume plus named volumes, each one host-side disk image under the state root — but left the image format, how big it is, when it is created, and whether freed space ever comes back. Left unanswered, the natural implementation preallocates a fixed image per volume, which turns "run five projects" into a static disk bill and contradicts the ceiling model ADR-0035 established for memory.

## Considered Options

- Preallocated fixed-size raw images.
- Copy-on-write images with snapshot support.
- Sparse raw images with a declared virtual size and discard-based reclamation.

## Decision Outcome

Chosen option: sparse raw with discard-based reclamation — disk behaves exactly like memory, so one mental model covers both.

- Format is raw, provisioned sparsely. A copy-on-write format buys snapshots and backing chains, none of which vivarium promises; it costs a metadata layer on every write and makes "how much is this really using" a tool call instead of a file stat.
- Declared size is a virtual ceiling. An undeclared volume takes the default in [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md). Allocation follows use.
- Creation is lazy: the image is materialized on the first `viv start` that needs it, not at declaration, and the filesystem is created with lazy initialization so a large virtual size costs almost nothing.
- Reclamation is periodic, not continuous. Guests do not mount with continuous discard — it puts the cost on every delete. A scheduled in-guest trim, plus `viv volume trim` on demand, punches the freed ranges back out of the host image.
- Volumes grow only. A ceiling may be raised between boots; it is never lowered in place.

## Consequences

- Good: a generous default costs nothing until used; `volume list` can show allocated against virtual.
- Bad: a sparse image can meet host exhaustion as a guest I/O error, so free space on the state filesystem must be probed.
- Bad: no snapshots, and reclamation lags until a trim runs.

## Status

Accepted

Amended by ADR-0049 — the backend version whose block device passes discard through to the host image is settled by the lockfile, so it is an evaluation-time assertion rather than the `doctor` probe this decision originally relied on.

Amends [`ADR-0019-volume-model.md`](./ADR-0019-volume-model.md), which keeps ownership of the volume model itself. Applied in [`../reference/spec/06-workspace-and-project-environment.md`](../reference/spec/06-workspace-and-project-environment.md) and [`../reference/spec/17-resources-and-capacity.md`](../reference/spec/17-resources-and-capacity.md).

The sparse-image promise holds, and it is now a number — measured 2026-08-05. A guest wrote 1 GiB into the store volume, deleted it, and ran `fstrim`; the host image's allocated blocks fell by 801,124,352 bytes (764.0 MiB). `fstrim` → `VIRTIO_BLK_T_DISCARD` → `fallocate(PUNCH_HOLE)` returns blocks end to end on real hardware. Every layer of that chain was already verified in upstream source; this closes the last open leg of the volume model with the number that was missing.

Measured on a plain file, deliberately, because the obvious experiment could not answer it. Running the trim after a store collection produced a null result — but the collection had freed nothing, and a discard can only return what the filesystem freed, so that null said nothing about discard. Splitting the two turned one ambiguous silence into two findings with two homes. The metric is the host image's allocated blocks, never `fstrim`'s own report, which is the length of the ranges handed to the kernel. Registered in [`../reference/microvm-verification-harness.md`](../reference/microvm-verification-harness.md).

Note this measures trim — extents freed inside a retained image — and not prune, which removes whole orphan images.
