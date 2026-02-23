# Visage — NixOS package derivation
#
# Build from repo:
#   nix build .#visage   (once wired into flake.nix)
#
# For nixpkgs submission, replace src/cargoLock with fetchFromGitHub + cargoHash.

{ lib
, rustPlatform
, pkg-config
, pam
, dbus
}:

rustPlatform.buildRustPackage {
  pname = "visage";
  version = "0.2.0";

  src = lib.cleanSource ../..;

  cargoLock.lockFile = ../../Cargo.lock;

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ pam dbus ];

  postInstall = ''
    # PAM module (cdylib — not installed by cargo install)
    install -Dm755 target/release/libpam_visage.so \
      $out/lib/security/pam_visage.so

    # D-Bus system bus policy
    install -Dm644 packaging/dbus/org.freedesktop.Visage1.conf \
      $out/share/dbus-1/system.d/org.freedesktop.Visage1.conf

    # systemd units
    install -Dm644 packaging/systemd/visaged.service \
      $out/lib/systemd/system/visaged.service
    install -Dm644 packaging/systemd/visage-resume.service \
      $out/lib/systemd/system/visage-resume.service
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
