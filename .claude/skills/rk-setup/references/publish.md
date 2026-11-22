# Task: publish

Publish the first version and prove the automated path: `rk guide setup` §5 to §8. Almost every step is the operator's, because the registry exposes no API for the token or the trusted publisher, the first upload is permanent, and the credentials are the operator's. Sequence the steps, print each one with the binding's walkthrough, and check each one by reading the registry or the forge back.

## Pre-flight

1. Read `rk binding <tech>`. Where the driver publishes to no registry, or `rk profile --target .` reports `registry: none`, skip §5, §6, and §8, and keep §7. Where the binding registers a pending trusted publisher for a name the registry does not hold yet, as PyPI does, skip §5 and take §6 first.
2. Run `rk setup status --target . --json` and `rk setup check --target . --json`. Read the `milestone` events: `first-version-published` and `automated-release-proven` read from the registry and the forge, and each registry form no API exposes reads `operator`, with its read-back check.
3. Resume at the first milestone not yet observed. Nothing is remembered between runs.

## Execution

In the runbook's order:

1. §5, the first version by hand. Take 5a: a clean tree, `HEAD` equal to the remote trunk, and the version read. Ask, stated plainly, before anything uploads: the upload is permanent, it claims the package name for good, and no version can ever be withdrawn or overwritten. Then print 5b, the bootstrap token, and 5c, the publish, for the operator. Never handle the token. Read the version back from the registry.
2. §6, the trusted publisher and the revocation of the bootstrap token: the operator's, with the binding's form walkthrough and its read-back check.
3. §7. Take 7a, the protected path, then 7b, the publish workflow ran on the trunk, then 7c, a release request is open. 7d is one release end to end, which the `rk-release` skill owns: stop with "Next: cut one release with the `rk-release` skill". The next run of this skill resumes at 7e from observation.
4. 7e, the refresh of the proof the landing wrote in 4c: `rk setup status --target .`, and where it reads anything but `compatible`, `rk setup checkpoint --target .` on a host that holds the bot key, committed through the trunk's one path.
5. §8, require trusted publishing: the operator's, and only after §7 proved one release through it.

Take the provenance the channel offers by default, with no signing scheme of your own. Where the channel offers none, say so in the report.

## Post-flight

1. `rk setup status --target . --json` reads the proof `compatible`, and `rk status --target . --json` reads `setup_proof` `compatible`. An `absent` or `stale` proof routes back to 7e.
2. `rk status --target . --json` reports `classification` `current`.
3. Keep each credential step under "Still the operator's" until its read-back passed.
