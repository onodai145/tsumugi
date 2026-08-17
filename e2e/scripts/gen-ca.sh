#!/usr/bin/env bash
# CA・サーバー証明書を初回のみ生成する。既に存在する場合は何もしない（冪等）。
set -euo pipefail
CERT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/certs"
mkdir -p "$CERT_DIR"

if [[ -f "$CERT_DIR/ca.pem" ]]; then
  echo "gen-ca: already exists, skip: $CERT_DIR/ca.pem"
  exit 0
fi

openssl genrsa -out "$CERT_DIR/ca-key.pem" 4096
openssl req -x509 -new -nodes -key "$CERT_DIR/ca-key.pem" -sha256 -days 3650 \
  -subj "/CN=tsumugi-e2e-test-CA" -out "$CERT_DIR/ca.pem"

openssl genrsa -out "$CERT_DIR/misskey.local-key.pem" 2048
openssl req -new -key "$CERT_DIR/misskey.local-key.pem" \
  -subj "/CN=misskey.local" -out "$CERT_DIR/misskey.local.csr"

cat > "$CERT_DIR/misskey.local.ext" <<EOF
subjectAltName = DNS:misskey.local
EOF

openssl x509 -req -in "$CERT_DIR/misskey.local.csr" \
  -CA "$CERT_DIR/ca.pem" -CAkey "$CERT_DIR/ca-key.pem" -CAcreateserial \
  -out "$CERT_DIR/misskey.local.pem" -days 3650 -sha256 \
  -extfile "$CERT_DIR/misskey.local.ext"

rm -f "$CERT_DIR/misskey.local.csr" "$CERT_DIR/misskey.local.ext"
echo "gen-ca: generated $CERT_DIR/ca.pem and $CERT_DIR/misskey.local.pem"
