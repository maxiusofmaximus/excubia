# Credenciales: qué se automatiza y qué no

Este proyecto tiene una credencial de firma para la app Android. Es un secreto
que hay que proteger, y se genera con un script.

| Credencial | ¿Automatizable? | Cómo |
|---|---|---|
| Key de firma Android | **Sí, entera** | `tooling/setup-signing.ps1` |

---

## 1. Key de firma Android — automatizada

Ya está hecha. `keystore.properties` y `keystores/release.jks` existen, y el APK
de release está firmado con v2 y v3, verificado con `apksigner`.

Para rehacerla desde cero:

```powershell
cd <ruta-del-repo>
.\tooling\setup-signing.ps1
```

**El script se niega a sobrescribir una clave existente.** No es prudencia
decorativa: si una release ya salió firmada con esa clave, Android identifica las
actualizaciones por la clave de firma, así que una clave nueva produce una app
distinta instalada en paralelo en vez de una actualización, y la instalada queda
inaccesible sin restauración de fábrica. Para empezar de cero hay que borrar el
fichero a mano.

El password se genera, no se teclea, y **no se imprime por pantalla**. Solo
aparece en `keystore.properties`, que está en `.gitignore`. Así una captura de
esta terminal o una transcripción de la sesión no contienen el secreto.

### Tu única decisión: la copia de seguridad

Esto no lo puede hacer un agente por ti, porque el destino es una decisión sobre
dónde vas a seguir teniendo algo dentro de cinco años:

- `keystores\release.jks`
- `keystore.properties`

Los dos juntos, o ninguno sirve: el binario sin las contraseñas es inútil.

### Detalle que salió al verificar

`apksigner` reporta `v1: false` y `v2: true`, `v3: true`. Es lo correcto: el
esquema v1 (JAR signing) solo hace falta por debajo de API 24, y el minSdk de
esta app es 26. Forzarlo añadiría firma sin propósito.

