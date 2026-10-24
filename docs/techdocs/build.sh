#!/usr/bin/env bash
# Prepara docs/techdocs/src para mdBook. Los contratos, las RFC y los demás documentos se
# copian desde su sitio original (core/docs y apk/docs): no hay copias versionadas. Solo se
# edita la copia generada; los originales no cambian.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
src="$root/docs/techdocs/src"

rm -rf "$src"
mkdir -p "$src/contracts" "$src/rfc" "$src/apk"

cp "$root/docs/techdocs/pages/"*.md "$src/"
cp "$root/core/docs/contracts/"*.md "$src/contracts/"
cp "$root/core/docs/rfc/"*.md "$src/rfc/"
cp "$root/core/docs/security-review.md" "$root/core/docs/PARITY-GATEWAY.md" "$src/"
cp "$root/apk/docs/migration-from-kotlin.md" "$src/apk/"

# Enlaces a ficheros que no se publican: se quita el enlace y queda el texto.
# core/docs/core/HOOKS.md no existe en el árbol, y roadmap.md no se publica.
sed -i -E 's#\[([^]]*)\]\(\.\./core/HOOKS\.md\)#\1#g; s#\[([^]]*)\]\(\.\./roadmap\.md\)#\1#g' \
  "$src"/*.md "$src"/*/*.md

# Un texto entre <> sin cerrar hace que mdBook avise: en la copia se escapa como texto.
sed -i 's#<computer>#\&lt;computer\&gt;#g' "$src/apk/migration-from-kotlin.md"

if grep -rn -E '\]\((\.\./)?(core/HOOKS|roadmap)\.md' "$src"; then
  echo "build.sh: quedan enlaces a ficheros no publicados" >&2
  exit 1
fi

echo "build.sh: $(find "$src" -name '*.md' | wc -l) documentos en $src"
