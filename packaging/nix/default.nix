# Visage — NixOS package derivation
#
# Usage (standalone):
#   nix build .#visage
#
# Usage (NixOS module — recommended):
#   imports = [ visage.nixosModules.default ];
#   services.visage.enable = true;
#
# For nixpkgs submission, replace src/cargoLock with fetchFromGitHub + cargoHash.

{ lib
, rustPlatform
, pkg-config
, pam
, dbus
, openssl
, onnxruntime
, substituteAll ? null
}:

rustPlatform.buildRustPackage {
  pname = "visage";
  # Sourced from [workspace.package] rather than hardcoded: this read "0.3.3"
  # while the workspace was at 0.3.6, so `nix build` produced an artifact whose
  # externally-visible version was three patch releases stale. Nothing catches
  # that — the derivation builds happily under any label.
  version = (lib.importTOML ../../Cargo.toml).workspace.package.version;

  src = lib.cleanSource ../..;

  cargoLock.lockFile = ../../Cargo.lock;

  # bindgenHook: `v4l2-sys-mit` (via visage-hw → camera capture) runs bindgen in
  # its build script, which dlopens libclang. Without it the build dies with
  # "Unable to find libclang ... set the LIBCLANG_PATH environment variable".
  # flake.nix already carries llvmPackages.libclang + LIBCLANG_PATH for the
  # devShell, so `cargo build` in a dev shell has always worked — but the
  # PACKAGE never had it, and nothing caught the difference because no consumer
  # referenced pkgs.visage: services.visage.enable is set on no host and the
  # package is in no systemPackages, so the derivation was never realised.
  # Found 2026-08-16 the first time a host tried to install it.
  nativeBuildInputs = [ pkg-config rustPlatform.bindgenHook ];
  # openssl: `ort` (ONNX Runtime) pulls `ureq` → `native-tls` → `openssl-sys`,
  # whose build script needs the system OpenSSL at link time (issue #38).
  buildInputs = [ pam dbus openssl ];

  # ort-sys downloads a prebuilt ONNX Runtime from cdn.pyke.io in its build
  # script, which the nix sandbox correctly blocks — so the package could never
  # build offline at all. Point it at nixpkgs' onnxruntime instead.
  ORT_LIB_LOCATION = "${onnxruntime}";

  # cargo test runs unit tests; integration tests require a camera + daemon
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    cargo test --workspace --lib
    runHook postCheck
  '';

  postInstall = ''
    # PAM module (cdylib — not installed by cargo install)
    install -Dm755 target/release/libpam_visage.so \
      $out/lib/security/pam_visage.so

    # D-Bus system bus policy
    install -Dm644 packaging/dbus/org.freedesktop.Visage1.conf \
      $out/share/dbus-1/system.d/org.freedesktop.Visage1.conf

    # systemd units — patch ExecStart to reference the Nix store path
    install -Dm644 packaging/systemd/visaged.service \
      $out/lib/systemd/system/visaged.service
    substituteInPlace $out/lib/systemd/system/visaged.service \
      --replace-fail "/usr/bin/visaged" "$out/bin/visaged"

    install -Dm644 packaging/systemd/visage-resume.service \
      $out/lib/systemd/system/visage-resume.service
    substituteInPlace $out/lib/systemd/system/visage-resume.service \
      --replace-fail "/usr/bin/systemctl" "systemctl"
  '';

  meta = with lib; {
    description = "Linux face authentication via PAM — persistent daemon, IR camera support, ONNX inference";
    longDescription = ''
      Visage is the Windows Hello equivalent for Linux. It authenticates sudo,
      login, and any PAM-gated service using your face — with sub-second response
      and no subprocess overhead. Built in Rust with a persistent daemon model,
      SCRFD face detection, and ArcFace recognition via ONNX Runtime.

      The default face authentication layer for Augmentum OS.
      Ships standalone on any Linux system.
    '';
    homepage = "https://github.com/sovren-software/visage";
    license = licenses.mit;
    maintainers = [ ];
    platforms = platforms.linux;
    mainProgram = "visage";
  };
}
