# Examples

Artifacts to copy into your config root. vivarium never installs them and never resolves a name to anything here: the config root is the whole search path, so an example does nothing until it is your file ([`../docs/decisions/ADR-0061-examples-ship-not-a-second-namespace.md`](../docs/decisions/ADR-0061-examples-ship-not-a-second-namespace.md)).

```console
$ cp -r examples/images examples/pieces examples/manifests "${XDG_CONFIG_HOME:-$HOME/.config}/vivarium/"
$ viv manifest list
$ viv config --manifest rust-web eval
```

Each file explains the convention it demonstrates in its own comments, because the convention is the reason to copy it: an image proposes with `mkDefault`, a shared piece either proposes or forces and carries no personal data, and the manifest is where your own decisions go. The rules behind all three are in [`../docs/reference/spec/04-composition-and-determinism.md`](../docs/reference/spec/04-composition-and-determinism.md). The base image is a directory carrying a `flake.nix` beside its module: that file is where its build inputs — `nixpkgs`, `microvm`, and anything else the image wants — become yours to redirect and `viv update`'s to move ([`../docs/decisions/ADR-0112-the-selected-image-carries-the-base-flake.md`](../docs/decisions/ADR-0112-the-selected-image-carries-the-base-flake.md)).

They age against upstream and carry no pin of their own, so treat them as documentation that can rot rather than as a supported library.
