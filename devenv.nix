{ pkgs, lib, config, inputs, ... }:

{
  packages = [
    pkgs.dioxus-cli
    pkgs.wasm-bindgen-cli_0_2_114
    # pkgs.lld
  ];
}
