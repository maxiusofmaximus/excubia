"""Ficha de entrada -> estado de Laya. El texto del correo solo va dentro de `untrusted`."""

import re

# Caracteres de control, formato bidi y de anchura cero: se sustituyen por espacio.
_CONTROL = re.compile("[\x00-\x1f\x7f-\x9f​-‏‪-‮⁦-⁩﻿]")
CHECK_KEYS = (
    "from_domain",
    "reply_to_domain",
    "return_path_domain",
    "reply_to_differs",
    "return_path_differs",
    "claims_attachment_but_none",
    "link_host_differs",
    "list_unsubscribe",
    "one_click_unsubscribe",
    "bulk",
)
AUTH_KEYS = ("spf", "dkim", "dmarc")


def render(ficha: object) -> dict | str:
    """Estado para Laya, o el código de error 400 si la ficha no sirve."""
    if not isinstance(ficha, dict):
        return "ficha_invalida"
    if not isinstance(ficha.get("from_address"), str) or not isinstance(ficha.get("subject"), str):
        return "ficha_invalida"
    return {
        "untrusted": {
            "from_name": _clean(ficha.get("from_name"), 120),
            "from_address": _clean(ficha["from_address"], 254),
            "subject": _clean(ficha["subject"], 300),
            "preview": _clean(ficha.get("preview"), 500),
        },
        "checks": _checks(ficha.get("checks")),
        "deterministic": _deterministic(ficha.get("deterministic")),
        "link_count": _count(ficha.get("link_count")),
        "contact_known": ficha.get("contact_known") if isinstance(ficha.get("contact_known"), bool) else None,
        "preferences": _clean(ficha.get("preferences"), 400) or None,
    }


def _clean(value: object, limit: int) -> str:
    if not isinstance(value, str):
        return ""
    return " ".join(_CONTROL.sub(" ", value).split())[:limit]


def _scalar(value: object) -> object:
    if isinstance(value, str):
        return _clean(value, 253)
    return value if value is None or isinstance(value, bool) else None


def _checks(raw: object) -> dict:
    if not isinstance(raw, dict):
        return {}
    out = {key: _scalar(raw[key]) for key in CHECK_KEYS if key in raw}
    auth = raw.get("auth")
    if isinstance(auth, dict):
        out["auth"] = {key: _scalar(auth.get(key)) for key in AUTH_KEYS}
    return out


def _deterministic(raw: object) -> dict | None:
    if not isinstance(raw, dict):
        return None
    signals = raw.get("signals")
    signals = signals if isinstance(signals, list) else []
    return {
        "level": _clean(raw.get("level"), 40),
        "signals": [_clean(signal, 120) for signal in signals[:20] if isinstance(signal, str)],
    }


def _count(value: object) -> int | None:
    if isinstance(value, int) and not isinstance(value, bool) and value >= 0:
        return value
    return None
