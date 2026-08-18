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
, fetchurl
, stdenvNoCC
, xz
, substituteAll ? null
}:

# ONNX Runtime, fetched hermetically instead of by ort-sys at build time.
#
# ort-sys's build script downloads this exact archive from cdn.pyke.io, which a
# sandboxed build correctly blocks — so `nix build .#visage` could never work
# offline. Pointing ORT_LIB_LOCATION at nixpkgs' onnxruntime does not help
# either: ort links STATICALLY by default and nixpkgs ships only .so, so the
# build fails with "could not link to the ONNX Runtime build in ...".
#
# So fetch the same archive ort-sys wants, pinned by hash. Version-matched by
# construction (1.23.2 — what ort 2.0.0-rc.11 expects), no version skew, and
# reproducible. The payload is a single flat libonnxruntime.a; it is installed
# to BOTH $out and $out/lib because ort-sys probes both.
#
# The archive is raw LZMA2, NOT an .xz container — `tar xf` reports "does not
# look like a tar archive". It needs `xz --format=raw --lzma2`.
let
  ortVersion = "1.23.2";
  ortStatic = stdenvNoCC.mkDerivation {
    pname = "onnxruntime-static-for-ort-sys";
    version = ortVersion;
    src = fetchurl {
      url = "https://cdn.pyke.io/0/pyke:ort-rs/ms@${ortVersion}/x86_64-unknown-linux-gnu.tar.lzma2";
      sha256 = "0px3pb5jv04y602458g1d6nr1q3rdiqdd62n5a0hgr5fm9cx0mwc";
    };
    nativeBuildInputs = [ xz ];
    dontUnpack = true;
    installPhase = ''
      runHook preInstall
      mkdir -p "$out/lib"
      # dict=64MiB is REQUIRED and is not the default. Plain `--lzma2` decodes
      # only ~8.9 MB of the ~93 MB payload and exits non-zero; piped into tar
      # with stderr hidden that looks like a successful extraction, because a
      # truncated ar archive still yields a plausible libonnxruntime.a.
      xz --format=raw --lzma2=dict=64MiB -dc "$src" | tar xf - -C "$out/lib"

      # Floor guard: silent under-extraction is the failure mode this hit, so
      # refuse a suspiciously small result rather than shipping a stub that
      # fails later at link time with an unrelated-looking error.
      test -f "$out/lib/libonnxruntime.a" || { echo "libonnxruntime.a missing"; exit 1; }
      sz=$(stat -c %s "$out/lib/libonnxruntime.a")
      if [ "$sz" -lt 80000000 ]; then
        echo "libonnxruntime.a is $sz bytes, expected ~93M — truncated decode"; exit 1
      fi

      ln -s "$out/lib/libonnxruntime.a" "$out/libonnxruntime.a"
      runHook postInstall
    '';
  };
in

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

  # See the ortStatic note above. This is what stops ort-sys reaching the network.
  ORT_LIB_LOCATION = "${ortStatic}";

  # cargo test runs unit tests; integration tests require a camera + daemon
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    cargo test --workspace --lib
    runHook postCheck
  '';

  postInstall = ''
    # PAM module (cdylib — not installed by cargo install).
    #
    # Located rather than hardcoded: this said `target/release/`, but current
    # rustPlatform.buildRustPackage passes --target, so cargo emits to
    # target/<triple>/release/ and the install failed with
    #   install: cannot stat 'target/release/libpam_visage.so'
    # Hardcoding the triple instead would just move the breakage to the next
    # nixpkgs change, so find it and fail loudly if it is absent or ambiguous.
    # Match the canonical `release/` segment exactly. A bare -name found three
    # copies in turn, each of which the guard refused rather than guessing:
    #   release/deps/libpam_visage.so   raw compilation artifact
    #   release-tmp/libpam_visage.so    buildRustPackage's staging dir
    #   release/libpam_visage.so        the one we want
    pam_so=$(find target -type f -path '*/release/libpam_visage.so' -print)
    n=$(printf '%s\n' "$pam_so" | grep -c .)
    if [ "$n" -ne 1 ]; then
      echo "expected exactly 1 libpam_visage.so under target/ (excluding deps/), found $n:" >&2
      printf '  %s\n' $pam_so >&2
      exit 1
    fi
    install -Dm755 "$pam_so" $out/lib/security/pam_visage.so

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
