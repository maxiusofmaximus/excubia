# RFC: cifrado en reposo del historial (`chat.db`) — la «bóveda»

Estado: DISEÑO aprobado por el Master (2026-11-18: «ciframos chat.db… como VeraCrypt, con datos aleatorios de mi ratón»). Implementación en fases V1–V3.
Hallazgo que lo origina: F4.2 (`docs/security-audit/D4-datos-en-reposo.md`, privado).

## Qué protege y qué no
Protege: `chat.db` (y sus `-wal`/`-shm`) contra quien copie los archivos sin sesión de Windows: copia de seguridad robada, disco extraído, otro usuario del PC, malware sin acceso a la sesión del usuario.
No protege: malware que corre **como el mismo usuario** con la app abierta (puede pedir a Credential Manager la clave igual que nosotros) ni memoria del proceso. Eso es ATT2b/D5, no cifrado en reposo.

## Cifrado: SQLCipher 4 (AES-256)
- `rusqlite` con `bundled-sqlcipher-vendored-openssl`: SQLCipher 4.x + OpenSSL compilado estático. AES-256-CBC por página de 4 KiB con IV aleatorio por página y **HMAC-SHA512 por página** (cifrado autenticado: detecta manipulación, cosa que XTS de VeraCrypt no hace).
- Clave dada como **clave cruda** (`PRAGMA key = "x'<64 hex>'"`): SQLCipher no aplica su PBKDF2 porque la clave ya es de 256 bits aleatorios; evita 256 000 iteraciones en cada apertura.
- Licencias: SQLCipher (BSD), OpenSSL (Apache-2.0): compatibles con el repo (Apache-2.0). Añadir a `NOTICE`.
- Compilar requiere Perl completo y compilador C (en Windows: Strawberry Perl + MSVC). Los runners de GitHub ya los traen.

## Clave maestra con entropía del usuario (inspirado en VeraCrypt)
VeraCrypt mezcla movimientos del ratón en un *pool* aleatorio y lo usa para generar la clave maestra del volumen. Hacemos lo mismo:

1. **Recolección** (escritorio, pantalla «Protege tu historial»): se muestrean `mousemove` (x, y, `performance.now()` en microsegundos y el delta). Barra de progreso hasta ≥ 2 048 muestras (≈ 10 s de movimiento). Se acumulan en un acumulador hash-chain (`estado = SHA-512(estado ‖ muestra)`), no por concatenación.
2. **Generación** (host, nunca en la WebView): `clave = HKDF-SHA-512( OS_CSPRNG(64 B) ‖ pool_ratón ‖ marca_tiempo )` → 32 bytes. El generador del sistema (`BCryptGenRandom`) siempre participa: **la entropía del ratón solo puede sumar, nunca restar** (si el ratón fuera predecible, la clave sigue siendo tan fuerte como el RNG del sistema). No se cuenta como más de ~1 bit por muestra.
3. Si no hay ratón (host sin interfaz, `excubia-host vault init --no-mouse`), solo RNG del sistema. Es igual de seguro en la práctica; la entropía del usuario es defensa en profundidad y transparencia.

Honestidad técnica: el RNG del sistema operativo ya es criptográficamente suficiente; el ratón añade garantía contra un fallo del RNG y da la «ceremonia» de VeraCrypt. No añade bits que el atacante tenga que adivinar si el RNG está sano.

## Dónde vive la clave (la «cabecera»)
- Credential Manager de Windows, objetivo `excubia/chat-db-key` (DPAPI atada al usuario de Windows), igual que la clave del correo (`SecretStore`). Nunca en disco junto a la base.
- **Frase de recuperación**: al crear la clave se muestra **una vez** una frase de 24 palabras (BIP-39 de la clave de 256 bits) para guardarla fuera del PC. Sin ella, si se pierde el perfil de Windows o se reinstala, el historial es irrecuperable (como VeraCrypt sin contraseña/disco de rescate). `excubia-host vault recover` la acepta.
- Contraseña opcional (VeraCrypt-style): **no en V1 ni por defecto** (el host arranca sin supervisión y sirve al móvil). Decidido por el Master (2026-11-18) como **función opcional para quien maneje datos de entidades bancarias o similares**; él mismo no la usa. Ver V4.

## Migración (regla: probar en base vieja, pasos numerados)
Al arrancar el host:
1. Detectar el estado por la cabecera del archivo: `SQLite format 3\0` = texto claro; otra cosa = cifrada.
2. Si hay clave en Credential Manager y la base está en claro: copia de respaldo `chat.db.pre-vault` (fuera del directorio de datos, en `.excubia-backups`), `ATTACH` de un archivo nuevo con `KEY`, `sqlcipher_export`, comprobar `PRAGMA integrity_check`, número de filas por tabla idéntico y `user_version`, cerrar, intercambiar archivos con `rename` atómico. Borrar los `-wal/-shm` viejos. El respaldo en claro se conserva hasta que el Master lo borre (`vault finish`), no antes.
3. Si no hay clave y la base está en claro: el host funciona como hoy y avisa (`vault: unprotected`) al escritorio, que ofrece la pantalla de entropía.
4. Si la base está cifrada y no hay clave: **el host no arranca el chat** y lo dice claro (no crea una base nueva vacía encima).

## Superficie y fases
- **V1 (núcleo):** SQLCipher compilando en todo el workspace; `persist::vault` (pool de entropía, HKDF, frase BIP-39, almacén de clave); `chat::schema::open` acepta clave; migración con pruebas (base vieja v28 con datos, base corrupta, base ya cifrada, clave equivocada, interrumpida a medias).
- **V2 (host):** `POST /v1/vault/init` (ADMIN, cuerpo con el pool del ratón), `GET /v1/vault/status`, CLI `excubia-host vault init|status|recover|finish`, arranque con migración, auditoría (`vault_initialised`, `vault_migrated`).
- **V3 (escritorio):** pantalla de recolección con barra, muestra de la frase de recuperación y confirmación («la guardé»), estado en Ajustes → Seguridad.
- **V4 (bloqueo con contraseña, opcional, apagado por defecto):** `excubia-host vault lock-set|lock-clear|unlock` y interruptor en Ajustes → Seguridad. Con el bloqueo activo la clave maestra deja de estar en Credential Manager: se guarda **envuelta** en un archivo `vault.key` (AES-256-GCM) bajo una KEK = Argon2id(contraseña, sal de 16 B; memoria ≥ 64 MiB, 3 pasadas, 4 carriles, parámetros guardados en el archivo para poder subirlos) mezclada con un secreto de Credential Manager (así copiar `vault.key` a otro PC o usuario no basta, y adivinar la contraseña exige también la sesión). El host arranca **bloqueado**: la API devuelve `423 locked` en las rutas del chat y `/v1/vault/status` dice `locked`; el escritorio muestra la pantalla de contraseña y manda `POST /v1/vault/unlock` (límite de intentos con espera creciente, auditado). Cambiar o quitar la contraseña pide la actual; la frase de recuperación sigue sirviendo si se olvida. Coste honesto: sin la contraseña el móvil no puede leer chats hasta que alguien desbloquee en el PC.
- Después: `mail.db` (ledger), `devices.json` y adjuntos locales con el mismo mecanismo o `AES-GCM` de archivo.

## Fuera de alcance / riesgos
- Rendimiento: ~5–15 % más lento en E/S de la base (irrelevante para un historial de chat).
- La búsqueda FTS5 funciona igual sobre SQLCipher (verificar en V1: `ensure_search`).
- Compilación: +3–8 min en frío por OpenSSL; el binario crece ~3 MB.
- La clave de 256 bits protege más que la contraseña de cualquier usuario; el punto débil real pasa a ser la sesión de Windows y la frase de recuperación guardada.
