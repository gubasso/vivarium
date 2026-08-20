# Scenarios

The method for every row in [`../README.md`](../README.md), and the evidence behind every verdict that needed more than its symbol. One file per scenario, named for the row it decides. A method is identical for all four subjects; a subject section says what one of them did, and links back to nothing — the capability name in the table is what links here.

Every subject section is read at that subject's fixed setup:

- `vivarium` — its one microVM
- `flake-pilot` — `firecracker-pilot`, at the upstream `claude` firecracker registration
- `glaipnir` — the libkrun microVM
- `podman` — `podman run --runtime krun`

A file carries a subject section only where the verdict needed more than its symbol, so the four are not all present everywhere. Which of the tables in [`../README.md`](../README.md) a scenario backs is legible there, where every capability name links to the file that decides it.

## Worked examples

A scenario carries a practical example where the command or the configuration text is itself the evidence, and not where the prose already carries the verdict. An example is transcribed from a source [`../sources.md`](../sources.md) pins, never invented to illustrate a point.

Two rules hold wherever one appears:

- A vivarium answer that is specified rather than built is marked `*`, and [`../../implementation-status.md`](../../implementation-status.md) is the source of truth for which is which.
- No command is shown that would refuse. A refusal is a verdict the prose states; a transcript of it teaches nothing the sentence does not.
