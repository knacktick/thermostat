{
  description = "Firmware for the Sinara 8451 Thermostat";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        overlays = [ (import rust-overlay) ];
      };

      rust = pkgs.rust-bin.stable."1.66.0".default.override {
        extensions = [ "rust-src" ];
        targets = [ "thumbv7em-none-eabihf" ];
      };
      rustPlatform = pkgs.makeRustPlatform {
        rustc = rust;
        cargo = rust;
      };

      thermostat = rustPlatform.buildRustPackage {
        name = "thermostat";
        version = "0.0.0";

        src = self;
        cargoLock = {
          lockFile = ./Cargo.lock;
          outputHashes = {
            "stm32-eth-0.2.0" = "sha256-48RpZgagUqgVeKm7GXdk3Oo0v19ScF9Uby0nTFlve2o=";
          };
        };

        nativeBuildInputs = [ pkgs.llvm ];

        buildPhase = ''
          cargo build --release --bin thermostat
        '';

        installPhase = ''
          mkdir -p $out $out/nix-support
          cp target/thumbv7em-none-eabihf/release/thermostat $out/thermostat.elf
          echo file binary-dist $out/thermostat.elf >> $out/nix-support/hydra-build-products
          llvm-objcopy -O binary target/thumbv7em-none-eabihf/release/thermostat $out/thermostat.bin
          echo file binary-dist $out/thermostat.bin >> $out/nix-support/hydra-build-products
        '';

        dontFixup = true;
        auditable = false;
      };

      pytec = pkgs.python3Packages.buildPythonPackage {
        pname = "pytec";
        version = "0.0.0";
        src = "${self}/pytec";

        propagatedBuildInputs =
          with pkgs.python3Packages; [
            numpy
            matplotlib
          ];
      };
    in
    {
      packages.x86_64-linux = {
        inherit thermostat pytec;
        default = thermostat;
      };

      hydraJobs = {
        inherit thermostat;
      };

      devShells.x86_64-linux.default = pkgs.mkShellNoCC {
        name = "thermostat-dev-shell";
        packages =
          with pkgs;
          [
            rust
            llvm
            openocd
            dfu-util
            rlwrap
          ]
          ++ (with python3Packages; [
            numpy
            matplotlib
          ]);
      };

      formatter.x86_64-linux = nixpkgs.legacyPackages.x86_64-linux.nixfmt-rfc-style;
    };
}
