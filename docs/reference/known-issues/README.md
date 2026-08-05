# Known issues

This registry covers bugs in external systems that vivarium integrates or tests. It begins empty; do not invent a case to populate it. Create the machine-readable registry with the first real case, when a check has a consumer for it.

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

| id | status | severity | affected tests | external system | upstream reference | mask |
| -- | ------ | -------- | -------------- | --------------- | ------------------ | ---- |

## Resolved

| id | summary | external system | upstream reference |
| -- | ------- | --------------- | ------------------ |
