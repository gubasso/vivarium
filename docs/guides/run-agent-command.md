# Run a command through the guest agent

> Design-intent walkthrough — not yet working. This guide describes the target experience. None of these commands run today; vivarium is at the design stage. For what is actually implemented, see [`../reference/implementation-status.md`](../reference/implementation-status.md), which is the source of truth for status. Read this as the north star the implementation aims at.

Use `viv exec` for a non-interactive command whose arguments and result must cross the guest-agent boundary predictably. The complete grammar and transport behavior live in [exec and shell](../reference/spec/12-exec-and-shell.md), and [the decision that fixed the control transport and the exec contract](../decisions/ADR-0016-guest-control-transport-and-exec-contract.md) explains why the status boundary sits where it does.

## Run ordinary commands

```console
$ viv exec -- true
$ viv exec -- false
$ viv exec -- sh -lc 'exit 42'
```

Place the guest command after the separator. Add terminal or environment options only on the vivarium side of that boundary.

## Handle outcomes

Treat a successfully started command as a guest-process result. Use the [exit-code matrix](../reference/spec/14-exit-codes.md) to distinguish failures that happen before the guest process starts or while its status is transported.

## Acceptance coverage

Three trials cover this. `workflow_06_exec_usage_surface`, in [`local_workflows.rs`](../../tests/local_workflows.rs), checks the usage errors that are answered before anything boots. In [`boot_workflows.rs`](../../tests/boot_workflows.rs), `workflow_06_exec_exit_code_propagation` checks ordinary statuses, signal termination, and the argument boundary in a running guest, and `workflow_06_shell_interactive_session` drives `viv shell` under a real terminal for the parts only a terminal shows — the size the guest receives before the shell starts, job control, a window resize reaching the guest, and the host terminal restored after the session is signalled. [`../../tests/host/exec-and-shell-check`](../../tests/host/exec-and-shell-check) is the runbook that drives all three on a capable host.
