# Excubia para Android

App móvil de Excubia (Flutter, con un núcleo Rust enlazado por
`flutter_rust_bridge`): chat con Alfred, tu equipo de agentes, agenda, correo
con borradores y avisos cuando algo necesita tu aprobación.

Se empareja con el host de Excubia de tu PC escaneando un QR y comparando un
código corto. Cada petición va firmada y la conexión usa TLS con la huella del
host fijada; no se expone ningún puerto a internet.

## Estructura

```
mobile/      app Flutter: lib/ (interfaz), rust/ (núcleo), android/
docs/        contratos (docs/contracts/), hoja de ruta, revisión de seguridad
tooling/     scripts de apoyo
```

## Compilar y probar

```bash
cd mobile
flutter pub get
flutter analyze
flutter test
cargo test --manifest-path rust/Cargo.toml
```

Las versiones firmadas se publican en GitHub Releases.
