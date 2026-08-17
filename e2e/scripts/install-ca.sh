#!/usr/bin/env bash
# 生成済みCAをOS信頼ストアへ登録する。既に登録済みならスキップする（冪等）。
set -euo pipefail
CERT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/certs"
CA_FILE="$CERT_DIR/ca.pem"
DEST="/usr/local/share/ca-certificates/tsumugi-e2e-test-ca.crt"

if [[ ! -f "$CA_FILE" ]]; then
  echo "install-ca: $CA_FILE not found. Run gen-ca.sh first." >&2
  exit 1
fi

NEW_FP="$(openssl x509 -in "$CA_FILE" -noout -fingerprint -sha256)"
if [[ -f "$DEST" ]]; then
  EXISTING_FP="$(openssl x509 -in "$DEST" -noout -fingerprint -sha256 2>/dev/null || true)"
  if [[ "$NEW_FP" == "$EXISTING_FP" ]]; then
    echo "install-ca: already installed with matching fingerprint, skip"
    exit 0
  fi
fi

sudo cp "$CA_FILE" "$DEST"
sudo update-ca-certificates
echo "install-ca: installed $DEST"
