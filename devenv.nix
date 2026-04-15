{ pkgs, lib, config, inputs, ... }:

{
  packages = [
    pkgs.wasm-bindgen-cli_0_2_114
    pkgs.binaryen # for wasm-opt

    # Leptos
    pkgs.trunk

    pkgs.lld
  ];
}
