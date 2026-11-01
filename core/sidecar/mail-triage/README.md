# Sidecar de triaje de correo (Laya) — modo sombra

Servidor HTTP local que envuelve el modelo de decisión [Laya](https://huggingface.co/convaiinnovations/laya)
para que el host lo consulte. **Modo sombra**: solo devuelve probabilidades; no mueve ni borra nada y no toca el buzón.

## Requisitos

- Python 3.10+ y [uv](https://docs.astral.sh/uv/). El venv se crea dentro de esta carpeta (`.venv/`).
- Opcional: GPU NVIDIA. Con la RTX 5070 (Blackwell) hace falta torch con CUDA 12.8 o superior; el
  `pyproject.toml` ya lo toma del índice `pytorch-cu128`.

## Crear el entorno

```powershell
cd sidecar\mail-triage
uv sync
```

`uv sync` instala `laya==0.4.18`, torch (cu128), pytest y psutil. Pesa varios GB (torch con CUDA).

## Arrancar

```powershell
uv run python server.py                                   # 127.0.0.1:8766, checkpoint typed-decisions
uv run python server.py --port 8800                       # otro puerto
uv run python server.py --checkpoint base                 # base | typed-decisions | multilingual | router
uv run python server.py --no-warmup                       # sin la inferencia de calentamiento previa a escuchar
```

Al arrancar hace una inferencia con una ficha sintética antes de escuchar (≈4 s en la RTX 5070): así la primera
llamada real tarda ≈0,2 s en vez de ≈3,8 s. Si falla, arranca igual con `warm: false` en `/health`.

Solo escucha en `127.0.0.1`. Cualquier otra dirección de enlace se rechaza por argumento. Rechaza también
cabeceras `Host` que no sean `127.0.0.1:<puerto>` o `localhost:<puerto>` (protección frente a DNS rebinding).

Endpoints:

- `GET /health` → `{"ok", "model", "checkpoint", "questions", "device", "loaded", "warm"}`.
- `POST /v1/triage` con `Content-Type: application/json` y `{"state": <ficha>}` (cuerpo ≤ 32 KiB).
  Respuesta: probabilidades por opción para cada pregunta de `questions.py` y `latency_ms`.

## Checkpoint

Por defecto, `typed-decisions` (ver tabla abajo). Se elige con `--checkpoint`:

| valor | qué carga |
|---|---|
| `typed-decisions` (defecto) | `convaiinnovations/laya`, subcarpeta `typed-decisions` |
| `base` | `convaiinnovations/laya`, raíz (inglés) |
| `multilingual` | `convaiinnovations/laya`, subcarpeta `multilingual` |
| `router` | `laya.Router`: elige inglés o multilingüe por idioma; nunca elige `typed-decisions` |

## Resultados (`eval.py`)

40 fichas **sintéticas** etiquetadas a mano (12 phishing, 28 legítimos difíciles, en español e inglés).
Ficha completa y preguntas actuales. Columnas: AUC; separación = media de P(phish) en phishing menos
en legítimos; acierto top-1 de `kind`; P y R con umbral 0,5; mejor F1 y su umbral; latencia mediana del
modelo (en proceso, sin HTTP).

| checkpoint | AUC | separación | kind@1 | P@0,5 | R@0,5 | F1* (umbral*) | ms |
|---|---|---|---|---|---|---|---|
| **typed-decisions** | **0,982** | **0,157** | **0,700** | 0,900 | 0,750 | **0,923** (0,467) | 77 |
| base | 0,552 | 0,010 | 0,475 | 0,300 | 1,000 | 0,500 (0,855) | 93 |
| multilingual | 0,717 | 0,044 | 0,325 | 0,308 | 1,000 | 0,645 (0,996) | 57 |
| router | 0,625 | 0,038 | 0,425 | 0,308 | 1,000 | 0,512 (0,871) | 93 |

Por idioma (AUC, ficha completa): typed-decisions 0,989 en español y 1,000 en inglés; base 0,506 en español
(al azar) y 0,667 en inglés; multilingual 0,800 y 0,699.

Variantes de ficha con `typed-decisions` (una cada vez): completa 0,982 · sin `deterministic` 0,976 ·
sin `checks` 0,994 · solo texto (`untrusted`) 0,762 · texto plano 0,554.
Variantes de preguntas: actual 0,982 · neutral en español 0,920 · neutral en inglés 0,905 · `phish` como
elección de dos opciones 0,887.

Lectura honesta: son indicios, no pruebas. Con 12 positivos, diferencias de 0,01–0,02 de AUC son ruido.
Las etiquetas de `checks` y `deterministic` las escribí yo y van en línea con la clase; por eso el dato de
«solo texto» (0,762) es el más realista como suelo. Las probabilidades de phishing con `typed-decisions`
caen entre 0,47 y 0,68: el ranking separa, pero el umbral 0,5 no sirve como veredicto. Las confianzas no
están calibradas (según el RFC, no se usan como umbral hasta calibrar con correos reales).

Matriz completa y salida cruda: `CORE-MAIL-F1A2-RESULT.md` (en `core/`).

## GPU o CPU

El dispositivo se elige solo: CUDA si torch la ve, si no CPU. `/health` indica cuál usa (`device`; `null`
con `router`, porque cada checkpoint elige el suyo).

## Modelo: tamaño y caché

- Repo `convaiinnovations/laya` (ModernBERT-large, 421M parámetros en `typed-decisions` y base). Cada
  subcarpeta pesa ~843 MB (`typed-decisions`) o ~680 MB (`multilingual`, incluye tokenizer de 34 MB). El
  sidecar baja solo los pesos, el tokenizer y el encoder que necesita.
- Caché de Hugging Face: `%USERPROFILE%\.cache\huggingface\hub` (cambiable con `HF_HOME`).
- Si el checkpoint ya está en caché, el arranque lo carga desde disco sin descargar nada. La primera vez sí descarga.
- `router`: la primera inferencia en frío tarda minutos (medido: 197 s), porque `Router` carga cada checkpoint
  al primer uso. No es apto para el host sin precarga.

## Pruebas

```powershell
uv run pytest -q          # incluye el modelo real (typed-decisions) si está en caché
uv run python smoke.py    # arranca el servidor dos veces (frío y caliente) y manda 14 fichas sintéticas
uv run python eval.py     # mide los 4 checkpoints con las 40 fichas (≈ varios minutos)
uv run python eval.py --checkpoint typed-decisions   # uno solo
```

## Archivos

- `server.py`: servidor HTTP (biblioteca estándar), validación de entrada y `--checkpoint`.
- `engine.py`: carga de Laya (checkpoint, caché), traducción de respuestas y `Engine.triage(state, questions)`.
- `questions.py`: preguntas tipadas, versión `mail_triage@p1` (sin cambios: ninguna variante ganó).
- `ficha.py`: ficha de entrada → estado de Laya; el texto del correo va solo en `untrusted`. Sin cambios.
- `synthetic.py`: `mk()` para fichas sintéticas y el conjunto etiquetado de 40 casos.
- `eval.py`, `eval_variants.py`, `metrics.py`: arnés de comparación y sus métricas (funciones puras).
- `smoke.py`: 14 fichas sintéticas contra el servidor real, con medidas de arranque, latencia y VRAM.
- Todo es sintético (dominios `example.*`); `tests/test_synthetic.py` lo vigila.
