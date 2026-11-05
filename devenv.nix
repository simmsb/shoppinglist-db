{ pkgs, lib, config, inputs, ... }:

{
  packages = with pkgs; [
    cargo-udeps
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
  };
}
