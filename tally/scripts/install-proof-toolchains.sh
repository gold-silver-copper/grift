#!/usr/bin/env bash
# Install the proof toolchains used by the tally and grift design prompts:
# Haskell (GHC + cabal), Lean 4 (elan), Rocq (+ Equations, via opam),
# Agda (+ standard library) and Bend 2.
#
# Written for macOS on Apple Silicon. It is idempotent: anything already
# installed is skipped. A failure in one toolchain does not stop the others.
# At the end it verifies every tool and can write a TOOLCHAINS.md report.
#
# Usage:
#   scripts/install-proof-toolchains.sh [options]
#
# Options:
#   --only LIST        comma-separated subset: haskell,lean,rocq,agda,bend
#   --skip LIST        comma-separated toolchains to skip
#   --report PATH      write a Markdown versions report to PATH
#   --update-zshrc     append the PATH/env lines to ~/.zshrc (once, marked)
#   --yes              don't ask before running Bend's downloaded installer
#   --dry-run          print what would run, change nothing
#   -h, --help         show this help
#
# Environment overrides:
#   OCAML_VERSION      OCaml compiler for the Rocq opam switch (default 4.14.2)
#   OPAM_SWITCH        name of the opam switch (default: rocq)
#   AGDA_STDLIB_DIR    where to unpack agda-stdlib if Homebrew lacks it
#                      (default: ~/agda-lib)

set -u
set -o pipefail

OCAML_VERSION="${OCAML_VERSION:-4.14.2}"
OPAM_SWITCH="${OPAM_SWITCH:-rocq}"
AGDA_STDLIB_DIR="${AGDA_STDLIB_DIR:-$HOME/agda-lib}"

ONLY=""
SKIP=""
REPORT=""
UPDATE_ZSHRC=0
ASSUME_YES=0
DRY_RUN=0

# ----------------------------------------------------------------------------
# Output helpers

if [ -t 1 ]; then
  BOLD=$'\033[1m'; RED=$'\033[31m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; RESET=$'\033[0m'
else
  BOLD=""; RED=""; GREEN=""; YELLOW=""; RESET=""
fi

step()  { printf '\n%s==> %s%s\n' "$BOLD" "$*" "$RESET"; }
info()  { printf '    %s\n' "$*"; }
warn()  { printf '    %swarning:%s %s\n' "$YELLOW" "$RESET" "$*"; }
fail()  { printf '    %serror:%s %s\n' "$RED" "$RESET" "$*"; }
ok()    { printf '    %sok:%s %s\n' "$GREEN" "$RESET" "$*"; }

# Run a command, or print it in --dry-run mode.
run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '    [dry-run] %s\n' "$*"
    return 0
  fi
  "$@"
}

# Run a shell snippet (for pipes), or print it in --dry-run mode.
run_sh() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '    [dry-run] %s\n' "$1"
    return 0
  fi
  bash -c "$1"
}

have() { command -v "$1" >/dev/null 2>&1; }

usage() { sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'; }

# ----------------------------------------------------------------------------
# Arguments

while [ $# -gt 0 ]; do
  case "$1" in
    --only) ONLY="$2"; shift 2 ;;
    --skip) SKIP="$2"; shift 2 ;;
    --report) REPORT="$2"; shift 2 ;;
    --update-zshrc) UPDATE_ZSHRC=1; shift ;;
    --yes) ASSUME_YES=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown option: %s\n\n' "$1"; usage; exit 2 ;;
  esac
done

# Is toolchain $1 selected by --only / --skip?
selected() {
  case ",$SKIP," in *",$1,"*) return 1 ;; esac
  [ -z "$ONLY" ] && return 0
  case ",$ONLY," in *",$1,"*) return 0 ;; esac
  return 1
}

RESULTS=""   # lines of "name|status|detail"
record() { RESULTS="${RESULTS}$1|$2|$3
"; }

# Lines to add to the shell rc so the tools are on PATH in new shells.
RC_LINES='# >>> proof toolchains (install-proof-toolchains.sh) >>>
[ -f "$HOME/.ghcup/env" ] && . "$HOME/.ghcup/env"
export PATH="$HOME/.elan/bin:$PATH"
command -v opam >/dev/null 2>&1 && eval "$(opam env --switch='"$OPAM_SWITCH"' 2>/dev/null)"
# <<< proof toolchains <<<'

# Make tools installed earlier in this run visible to later steps.
refresh_path() {
  [ -f "$HOME/.ghcup/env" ] && . "$HOME/.ghcup/env"
  case ":$PATH:" in *":$HOME/.elan/bin:"*) ;; *) PATH="$HOME/.elan/bin:$PATH" ;; esac
  for d in "$HOME/.bend/bin" "$HOME/.local/bin" "$HOME/.cargo/bin"; do
    [ -d "$d" ] && case ":$PATH:" in *":$d:"*) ;; *) PATH="$d:$PATH" ;; esac
  done
  if have opam && opam switch list --short 2>/dev/null | grep -qx "$OPAM_SWITCH"; then
    eval "$(opam env --switch="$OPAM_SWITCH" 2>/dev/null)" || true
  fi
  export PATH
}

# ----------------------------------------------------------------------------
# Preflight

preflight() {
  step "Preflight"
  if [ "$(uname -s)" != "Darwin" ]; then
    warn "this script targets macOS; other systems may need different package commands"
  fi
  [ "$(uname -m)" = "arm64" ] || warn "not Apple Silicon ($(uname -m)); continuing"
  if ! have brew; then
    fail "Homebrew is required: https://brew.sh"
    exit 1
  fi
  ok "Homebrew $(brew --version | head -1 | awk '{print $2}')"
  if have clang; then
    local major
    major=$(clang --version | head -1 | sed -E 's/.*clang version ([0-9]+).*/\1/')
    if [ "${major:-0}" -ge 14 ] 2>/dev/null; then
      ok "clang $major (Bend 2 needs 14+)"
    else
      warn "clang $major is older than 14; Bend 2 binaries may fail to build"
    fi
  else
    warn "no clang; run: xcode-select --install"
  fi
  local free_gb
  free_gb=$(df -g "$HOME" | awk 'NR==2 {print $4}')
  if [ "${free_gb:-0}" -lt 15 ] 2>/dev/null; then
    warn "only ${free_gb} GB free; the full install needs roughly 10-15 GB"
  else
    ok "${free_gb} GB free"
  fi
  refresh_path
}

# ----------------------------------------------------------------------------
# Haskell: ghcup, GHC, cabal

install_haskell() {
  step "Haskell (ghcup, GHC, cabal)"
  refresh_path
  if have ghc && have cabal; then
    ok "already installed: $(ghc --numeric-version 2>/dev/null) / cabal $(cabal --numeric-version 2>/dev/null)"
  else
    if ! have ghcup; then
      info "installing ghcup with the recommended GHC and cabal (non-interactive)"
      run_sh "curl --proto '=https' --tlsv1.2 -sSf https://get-ghcup.haskell.org | \
BOOTSTRAP_HASKELL_NONINTERACTIVE=1 BOOTSTRAP_HASKELL_GHC_VERSION=recommended \
BOOTSTRAP_HASKELL_CABAL_VERSION=recommended BOOTSTRAP_HASKELL_INSTALL_NO_STACK=1 sh" \
        || { fail "ghcup bootstrap failed"; record haskell FAILED "ghcup bootstrap"; return; }
    fi
    refresh_path
    run ghcup install ghc recommended --set || { fail "ghc install failed"; record haskell FAILED "ghc"; return; }
    run ghcup install cabal recommended --set || { fail "cabal install failed"; record haskell FAILED "cabal"; return; }
    refresh_path
  fi
  run cabal update || warn "cabal update failed (network?)"
  if [ "$DRY_RUN" -eq 1 ] || { have ghc && have cabal; }; then
    record haskell OK "$(ghc --numeric-version 2>/dev/null || echo '?') / cabal $(cabal --numeric-version 2>/dev/null || echo '?')"
    info "QuickCheck is installed per project by cabal; Liquid Haskell is optional (uses z3)"
  else
    record haskell FAILED "ghc/cabal not on PATH after install"
  fi
}

# ----------------------------------------------------------------------------
# Lean 4: elan, stable toolchain

install_lean() {
  step "Lean 4 (elan, stable toolchain)"
  refresh_path
  if ! have elan; then
    info "installing elan with the stable Lean toolchain"
    run_sh "curl -sSfL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh | \
sh -s -- -y --no-modify-path --default-toolchain stable" \
      || { fail "elan install failed"; record lean FAILED "elan-init"; return; }
    refresh_path
  else
    run elan toolchain install stable >/dev/null 2>&1 || true
    run elan default stable || warn "could not set the stable default"
  fi
  if [ "$DRY_RUN" -eq 1 ] || { have lean && have lake; }; then
    record lean OK "$(lean --version 2>/dev/null | head -1)"
    info "projects pin their own version in lean-toolchain; Mathlib (optional) via 'lake exe cache get'"
  else
    record lean FAILED "lean/lake not on PATH after install"
  fi
}

# ----------------------------------------------------------------------------
# Rocq + Equations: opam switch

opam_install_first() {
  # Install the first package in the argument list that opam knows about.
  local p
  for p in "$@"; do
    if opam show "$p" >/dev/null 2>&1; then
      run opam install -y "$p" && { echo "$p"; return 0; }
    fi
  done
  return 1
}

install_rocq() {
  step "Rocq + Equations (opam switch '$OPAM_SWITCH', OCaml $OCAML_VERSION)"
  if ! have opam; then
    run brew install opam || { fail "brew install opam failed"; record rocq FAILED "opam"; return; }
  fi
  if [ ! -d "$HOME/.opam" ]; then
    run opam init -y --bare || { fail "opam init failed"; record rocq FAILED "opam init"; return; }
  fi
  if ! opam switch list --short 2>/dev/null | grep -qx "$OPAM_SWITCH"; then
    info "creating the switch (compiles OCaml; several minutes)"
    run opam switch create "$OPAM_SWITCH" "ocaml-base-compiler.$OCAML_VERSION" -y \
      || { fail "opam switch create failed"; record rocq FAILED "switch"; return; }
  fi
  [ "$DRY_RUN" -eq 1 ] || eval "$(opam env --switch="$OPAM_SWITCH")"
  # The Rocq repository carries Equations and other released plugins.
  if ! opam repo list --short 2>/dev/null | grep -qx rocq-released; then
    run opam repo add rocq-released https://rocq-prover.org/opam/released --this-switch -y \
      || run opam repo add rocq-released https://coq.inria.fr/opam/released --this-switch -y \
      || warn "could not add the rocq-released repository; Equations may be unavailable"
  fi
  if have rocq || have coqc; then
    ok "Rocq already installed"
  else
    info "installing Rocq (compiles; typically 15-40 minutes)"
    if [ "$DRY_RUN" -eq 1 ]; then
      run opam install -y rocq-prover
    elif ! opam_install_first rocq-prover coq >/dev/null; then
      fail "neither rocq-prover nor coq could be installed"
      record rocq FAILED "rocq-prover"
      return
    fi
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    run opam install -y rocq-equations
  else
    local eq
    eq=$(opam_install_first rocq-equations coq-equations) \
      && ok "installed $eq" \
      || warn "Equations not installed; agents may still use plain Rocq"
  fi
  refresh_path
  local v
  v=$( (rocq --version 2>/dev/null || coqc --version 2>/dev/null) | head -1)
  if [ "$DRY_RUN" -eq 1 ] || [ -n "$v" ]; then
    record rocq OK "${v:-dry-run}"
  else
    record rocq FAILED "rocq/coqc not on PATH"
  fi
}

# ----------------------------------------------------------------------------
# Agda + standard library

agda_stdlib_works() {
  local dir
  dir=$(mktemp -d)
  printf 'module T where\nopen import Data.Nat using (ℕ)\nx : ℕ\nx = 1\n' > "$dir/T.agda"
  (cd "$dir" && agda T.agda >/dev/null 2>&1)
  local rc=$?
  rm -rf "$dir"
  return $rc
}

install_agda() {
  step "Agda + standard library"
  if ! have agda; then
    run brew install agda || { fail "brew install agda failed"; record agda FAILED "brew"; return; }
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    record agda OK "dry-run"
    return
  fi
  local av
  av=$(agda --numeric-version 2>/dev/null || agda --version | awk '{print $3}')
  if agda_stdlib_works; then
    ok "Agda $av with a working standard library"
    record agda OK "$av, stdlib working"
    return
  fi
  info "standard library not found; fetching the latest agda-stdlib release"
  local tag url
  tag=$(curl -fsSL https://api.github.com/repos/agda/agda-stdlib/releases/latest \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
  if [ -z "$tag" ]; then
    fail "could not determine the latest agda-stdlib release"
    record agda PARTIAL "$av, no stdlib"
    return
  fi
  url="https://github.com/agda/agda-stdlib/archive/refs/tags/$tag.tar.gz"
  mkdir -p "$AGDA_STDLIB_DIR" "$HOME/.agda"
  curl -fsSL "$url" | tar -xz -C "$AGDA_STDLIB_DIR" \
    || { fail "download of $url failed"; record agda PARTIAL "$av, no stdlib"; return; }
  local libfile
  libfile=$(find "$AGDA_STDLIB_DIR" -maxdepth 2 -name 'standard-library.agda-lib' | sort | tail -1)
  if [ -z "$libfile" ]; then
    fail "standard-library.agda-lib not found after unpacking"
    record agda PARTIAL "$av, no stdlib"
    return
  fi
  grep -qxF "$libfile" "$HOME/.agda/libraries" 2>/dev/null || echo "$libfile" >> "$HOME/.agda/libraries"
  grep -qx 'standard-library' "$HOME/.agda/defaults" 2>/dev/null || echo 'standard-library' >> "$HOME/.agda/defaults"
  if agda_stdlib_works; then
    ok "Agda $av with agda-stdlib $tag"
    record agda OK "$av, stdlib $tag"
  else
    warn "agda-stdlib $tag does not typecheck with Agda $av; pick a compatible release by hand"
    record agda PARTIAL "$av, stdlib $tag incompatible?"
  fi
}

# ----------------------------------------------------------------------------
# Bend 2: official installer, reviewed before running

install_bend() {
  step "Bend 2"
  refresh_path
  if have bend; then
    ok "already installed: $(bend --version 2>/dev/null | head -1)"
    record bend OK "$(bend --version 2>/dev/null | head -1)"
    return
  fi
  local script
  script=$(mktemp -t bend-install).sh
  if [ "$DRY_RUN" -eq 1 ]; then
    run curl -fsSL https://bend-lang.com/install.sh -o "$script"
    run sh "$script"
    record bend OK "dry-run"
    return
  fi
  curl -fsSL https://bend-lang.com/install.sh -o "$script" \
    || { fail "could not download https://bend-lang.com/install.sh"; record bend FAILED "download"; return; }
  info "downloaded installer: $script ($(wc -l < "$script" | tr -d ' ') lines, sha256 $(shasum -a 256 "$script" | cut -c1-16)…)"
  if [ "$ASSUME_YES" -ne 1 ]; then
    if [ -t 0 ]; then
      printf '    review it now? [Y/n] '
      read -r ans
      case "$ans" in n|N) ;; *) ${PAGER:-less} "$script" ;; esac
      printf '    run the Bend installer? [y/N] '
      read -r ans
      case "$ans" in y|Y) ;; *) info "skipped; run later with: sh $script"; record bend SKIPPED "not confirmed"; return ;; esac
    else
      info "not a terminal; skipping. Re-run with --yes, or run: sh $script"
      record bend SKIPPED "needs confirmation"
      return
    fi
  fi
  sh "$script" || { fail "Bend installer failed"; record bend FAILED "installer"; return; }
  refresh_path
  if have bend; then
    record bend OK "$(bend --version 2>/dev/null | head -1)"
    info "try: bend guide   (proofs: bend FILE.bend)"
  else
    warn "installer finished but 'bend' is not on PATH; check the installer's output for its location"
    record bend PARTIAL "installed, not on PATH"
  fi
}

# ----------------------------------------------------------------------------
# Shell rc and report

update_zshrc() {
  [ "$UPDATE_ZSHRC" -eq 1 ] || {
    step "Shell setup"
    info "add these lines to ~/.zshrc (or re-run with --update-zshrc):"
    printf '%s\n' "$RC_LINES" | sed 's/^/      /'
    return
  }
  step "Updating ~/.zshrc"
  if grep -q 'proof toolchains (install-proof-toolchains.sh)' "$HOME/.zshrc" 2>/dev/null; then
    ok "already present"
  elif [ "$DRY_RUN" -eq 1 ]; then
    info "[dry-run] would append the toolchain block to ~/.zshrc"
  else
    printf '\n%s\n' "$RC_LINES" >> "$HOME/.zshrc"
    ok "appended; open a new shell or run: source ~/.zshrc"
  fi
}

ver() { "$@" 2>/dev/null | head -1; }

write_report() {
  [ -n "$REPORT" ] || return 0
  step "Writing $REPORT"
  if [ "$DRY_RUN" -eq 1 ]; then info "[dry-run] skipped"; return 0; fi
  refresh_path
  mkdir -p "$(dirname "$REPORT")"
  {
    echo "# Toolchains"
    echo
    echo "Installed and verified by \`scripts/install-proof-toolchains.sh\` on $(date '+%Y-%m-%d %H:%M %Z')."
    echo "Machine: $(sw_vers -productName 2>/dev/null) $(sw_vers -productVersion 2>/dev/null), $(uname -m)."
    echo
    echo "| Tool | Version |"
    echo "|---|---|"
    echo "| Rust | $(ver rustc --version) |"
    echo "| cargo | $(ver cargo --version) |"
    echo "| GHC | $(ver ghc --version) |"
    echo "| cabal | $(ver cabal --version) |"
    echo "| Lean | $(ver lean --version) |"
    echo "| Lake | $(ver lake --version) |"
    echo "| Rocq | $( (rocq --version 2>/dev/null || coqc --version 2>/dev/null) | head -1) |"
    echo "| Equations | $(opam list --installed --short --columns=name,version rocq-equations coq-equations 2>/dev/null | tr '\n' ' ') |"
    echo "| Agda | $(ver agda --version) |"
    echo "| agda-stdlib | $(grep -h 'standard-library' "$HOME/.agda/libraries" 2>/dev/null | head -1) |"
    echo "| Bend | $(ver bend --version) |"
    echo "| z3 | $(ver z3 --version) |"
    echo "| clang | $(ver clang --version) |"
    echo
    echo "Install results:"
    echo
    printf '%s' "$RESULTS" | while IFS='|' read -r name status detail; do
      [ -n "$name" ] && echo "- $name: $status ($detail)"
    done
  } > "$REPORT"
  ok "wrote $REPORT"
}

# ----------------------------------------------------------------------------
# Main

preflight
selected haskell && install_haskell
selected lean && install_lean
selected rocq && install_rocq
selected agda && install_agda
selected bend && install_bend
update_zshrc
write_report

step "Summary"
status=0
printf '%s' "$RESULTS" | while IFS='|' read -r name status_ detail; do
  [ -n "$name" ] || continue
  case "$status_" in
    OK) ok "$name: $detail" ;;
    SKIPPED|PARTIAL) warn "$name: $status_ ($detail)" ;;
    *) fail "$name: $status_ ($detail)" ;;
  esac
done
case "$RESULTS" in *"|FAILED|"*) status=1 ;; esac
[ "$DRY_RUN" -eq 1 ] && info "dry run: nothing was changed"
exit $status
