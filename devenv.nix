{ pkgs, lib, config, inputs, ... }:

{
  packages = [
    # Dioxus
    pkgs.dioxus-cli
    pkgs.wasm-bindgen-cli_0_2_114
    pkgs.binaryen # for wasm-opt

    # pkgs.lld
  ];
}
