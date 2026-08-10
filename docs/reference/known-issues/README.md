# Known issues

This registry covers bugs in external systems that vivarium integrates or tests. The Markdown tables are the human status index; add a separate `registry.yaml` only when a deterministic consumer exists.

## Status values

- `open`: reproduced and awaiting investigation.
- `investigating`: actively being root-caused.
- `mitigated`: a permanent legitimate guard exists, but the upstream defect remains.
- `masked`: a temporary workaround exists and carries a revert trigger.
- `monitoring`: an upstream fix appears deployed and recurrence is being watched.
- `resolved`: the fix is confirmed and every temporary mask is removed.

## Lifecycle

Give each live case a stable `KI-NNNN` id and a directory containing its index, metadata, investigation, upstream escalation, any mask ledger, and evidence. Code or tests that suppress or expect the failure cite that id and the exact revert condition.

Expand the dossier while the issue is hot. When resolved, collapse it into one summary under `resolved/` containing the symptom, root cause, deployed fix and proof, and the recurrence signal. Version-control history retains the raw trail.

## Active

| id                             | status | severity | affected tests                                                        | external system | upstream reference                                     | mask |
| ------------------------------ | ------ | -------- | --------------------------------------------------------------------- | --------------- | ------------------------------------------------------ | ---- |
| [KI-0001](./KI-0001/README.md) | open   | high     | `store-pressure-collector-freed`, `store-pressure-collector-per-path` | Nix 2.34.8      | [nix#16269](https://github.com/NixOS/nix/issues/16269) | none |

## Resolved

| id                               | summary                                                                                                                                                | external system                                        | upstream reference                                                                      |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| [KI-0002](./resolved/KI-0002.md) | A guest `VSOCK_OP_SHUTDOWN` never reached the host peer as end of file, so a finished credential relay never unwound and its pool slot never refilled. | Cloud Hypervisor, broken through v52.0, fixed in v53.0 | [cloud-hypervisor#8372](https://github.com/cloud-hypervisor/cloud-hypervisor/pull/8372) |
