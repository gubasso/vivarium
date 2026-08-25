# Allowlist

Record whether destinations can be permitted by name rather than by address, and what a denied name answers.[^read]

1. Configure an allowlist naming one host.
2. Inside, resolve and connect to that host.
3. Resolve and connect to a host not on the list.
4. Record both answers, and whether a denial is distinguishable from a name that does not exist.

## vivarium

Yes: `egress.allow` names destinations as an exact name, `*.example.com` for exactly one further label, `**.example.com` for one or more, or a literal address or CIDR block, concatenating across composed pieces. vivarium's resolver is the only DNS the guest is given and is the enforcement point: an unmatched name is answered `REFUSED` and never forwarded, so it does not even leak upstream, and a matched name's addresses enter the filter before the reply reaches the guest. A denial is therefore distinguishable from a name that does not exist, which answers `NXDOMAIN`. An entry carries no port: the policy limits where data can go, not which port it leaves by.

An entry is a name, a wildcard, or a literal address, and the whole policy is four lines of the manifest:

```toml
[egress]
mode  = "allowlist"
allow = ["api.anthropic.com", "*.crates.io", "10.0.0.0/8"]
```

## bunkerbox

Yes, and it is the only other name-level mechanism in this set. `allow` in the runtime config lists hostnames; a project config may append to that list and may not shorten it, since upstream's position is that the packager sets the minimum.

```yaml
network: bridge
allow:
  - api.deepseek.com
```

Two enforcement points, resolved differently, and the difference from vivarium is in both.

For the guest, `iptables` resolves every listed hostname to its IPv4 addresses once, when the `BUNKERBOX-EGRESS` chain is built, and permits those addresses. DNS to the host's resolvers is permitted ahead of that, so an unlisted name still resolves — the query reaches upstream, and the denial arrives as a `REJECT` on the connection instead. That is distinguishable from a name that does not exist, and it is a weaker seal than answering the name `REFUSED`: the policy is addresses from then on, so a name whose addresses rotate stops working, one that shares an address with a listed name is reachable, and nothing re-resolves.

For a sandboxed passthrough command, the path is different and tighter. There is no direct network at all, only a relay to a host proxy which resolves the name at connection time and validates each concrete address against the policy before connecting. Exact names only, no wildcards, at either point.

[^read]: Read at `vivarium` `ceb0027` on 2026-08-18; `bunkerbox` `b7f14f3` on 2026-08-25.
