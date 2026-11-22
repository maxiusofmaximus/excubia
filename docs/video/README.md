# Vídeo de presentación de Excubia

Proyecto [HyperFrames](https://hyperframes.heygen.com): HTML y GSAP que se renderizan a vídeo.
El resultado publicado está en `docs/site/media/` (`excubia.mp4` y `excubia-poster.jpg`).

## Reconstruir

Desde esta carpeta (hace falta Node 22+, ffmpeg y Python 3):

```sh
python audio/make_audio.py                 # regenera assets/audio/*.mp3
pnpm dlx hyperframes@0.9.35 check
pnpm dlx hyperframes@0.9.35 render --quality delivery -o renders/excubia.mp4
ffmpeg -y -i renders/excubia.mp4 -c:v libx264 -preset slow -crf 22 -pix_fmt yuv420p \
  -af "loudnorm=I=-16:TP=-1.5:LRA=11" -ar 48000 -c:a aac -b:a 160k -movflags +faststart \
  ../site/media/excubia.mp4
ffmpeg -y -ss 3.5 -i ../site/media/excubia.mp4 -frames:v 1 -q:v 3 ../site/media/excubia-poster.jpg
```

## Origen y licencia de los recursos

| Recurso | Origen | Licencia |
|---|---|---|
| `assets/audio/music.mp3` | Síntesis propia con `audio/make_audio.py` (Python estándar, sin muestras) | CC0 1.0 |
| `assets/audio/whoosh-a.mp3`, `whoosh-b.mp3`, `pop.mp3`, `impact.mp3` | Síntesis propia con `audio/make_audio.py` | CC0 1.0 |
| `assets/img/icon.svg` | Icono del proyecto (`docs/site/icon.svg`) | La del repositorio |
| `assets/img/equipo.png`, `nueva-conversacion.png` | Capturas reales de la app de escritorio (perfil de prueba) | La del repositorio |
| `assets/img/chat-movil.png` | Captura real de la app Android, sin la barra de estado | La del repositorio |
| Tipografías Inter y JetBrains Mono | Incluidas en HyperFrames | SIL Open Font License 1.1 |
| GSAP 3.15 | Se carga solo al renderizar; no va en el vídeo ni en la web | Licencia estándar de GSAP |
