# Scenarios

The method for every row in [`../README.md`](../README.md), and the evidence behind every verdict that needed more than its symbol. One file per scenario, named for the row it decides. The method is identical for all four subjects; a subject section says what one of them did. Nothing here links back — the capability name in the table is what links in.

Every subject section is read at that subject's fixed setup:

- `vivarium` — its one microVM
- `flake-pilot` — both microVM routes: `firecracker-pilot` at the upstream `claude` firecracker registration, and `podman-pilot` at `--runtime krun` at the upstream `claude` `krun` registration
- `glaipnir` — the libkrun microVM
- `bunkerbox` — its one Kata container, at `io.containerd.kata.v2`

flake-pilot has one section per scenario carrying both routes, because they share a registration model and the interesting part is where they diverge. A section names the route whenever the verdict differs, and says so plainly when it does not.

Every method starts by setting the subject up the way its own documentation says to before first use — registering a flake, writing a manifest, whatever that tool calls it — and only then runs the steps. Configuration a user is told to do once is inside what the tool provides; a decision retyped at each start is not the same capability.

A file carries a subject section only where the verdict needed more than its symbol, so the four are not all present everywhere.

## Worked examples

A scenario carries a practical example where the command or the configuration text is itself the evidence, and not where the prose already carries the verdict. An example is transcribed from a source [`../sources.md`](../sources.md) pins, never invented to illustrate a point.

Two rules hold wherever one appears:

- A vivarium answer that is specified rather than built is marked `*`, and [`../../implementation-status.md`](../../implementation-status.md) is the source of truth for which is which.
- No command is shown that would refuse. A refusal is a verdict the prose states; a transcript of it teaches nothing the sentence does not.
