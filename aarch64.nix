{ pkgs ? <nixpkgs>, source ? ./., version ? "dev", crossSystem ? null }:

import ./default.nix {
    crossSystem = (import pkgs {}).lib.systems.examples.aarch64-multiplatform;
    source = source;
    version = version;
}
