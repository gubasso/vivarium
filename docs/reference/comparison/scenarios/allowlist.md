# Allowlist

Record whether destinations can be permitted by name rather than by address, and what a denied name answers.

1. Configure an allowlist naming one host.
2. Inside, resolve and connect to that host.
3. Resolve and connect to a host not on the list.
4. Record both answers, and whether a denial is distinguishable from a name that does not exist.

## vivarium

vivarium `ceb0027`, 2026-08-18. Yes: `egress.allow` names destinations as an exact name, `*.example.com` for exactly one further label, `**.example.com` for one or more, or a literal address or CIDR block, concatenating across composed pieces. vivarium's resolver is the only DNS the guest is given and is the enforcement point: an unmatched name is answered `REFUSED` and never forwarded, so it does not even leak upstream, and a matched name's addresses enter the filter before the reply reaches the guest. A denial is therefore distinguishable from a name that does not exist, which answers `NXDOMAIN`. An entry carries no port: the policy limits where data can go, not which port it leaves by. No other subject in this set has a name-level mechanism to compare.
