# Trust classification

Every subject here confines the software inside. Record whether any of them has an opinion about it — whether the tool knows that one workload is riskier than another, and does something different when the riskier one is named.[^read]

1. Name two different applications the tool can run, one of which its own material treats as riskier.
2. Start each by the ordinary invocation.
3. Record what differed, and at which verbs the difference appeared.

## vivarium

No, by design: vivarium's classification runs on artifacts rather than on the software they carry. The [personal-data-free-artifact rule](../../spec/08-invariants-and-guarantees.md) sorts an image or a piece into shared against personal and refuses a literal personal path in the former at exit `65`. That says something about where a definition may travel, and nothing about what the definition installs. Whether the tool should have an opinion about the application at all is the same unstated position [the credential-scoping row](./per-tool-credentials.md) turns on, logged as `Q-031` in [`open-questions.md`](../../../plan/open-questions.md).

## glaipnir

Yes, and it is the only one here. Five agents are trusted; `hermes-agent` is not, and naming it for a `build` or a `run` triggers an interactive disclaimer that exits on anything but yes. `clean` and `status` pass without prompting, so the classification costs nothing until it would matter.

## bunkerbox

No: every packaged tool is the same kind of thing to bunkerbox, and an image config has no trust attribute. The nearest surface points the other way — `profiles` is a judgement about which host binaries a build may reach, made per project by the person who owns the host, not a judgement about the agent that will call them.

[^read]: Read at `glaipnir` `21ef389` on 2026-08-18, recorded then in [`../walkthroughs.md`](../walkthroughs.md) and promoted to a row on 2026-08-25 when the admission rule relaxed to one subject. `bunkerbox` read at `b7f14f3` on 2026-08-25. The unlinked `flake-pilot` cells are absence claims derived from the registration schema already inventoried in [`../feature-sweep.md`](../feature-sweep.md).
