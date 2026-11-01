"""Variantes que mide eval.py: de ficha (qué se envía a Laya) y de preguntas (cómo se pregunta)."""

from typing import Callable, NamedTuple

from questions import QUESTIONS


def _sin(state: dict, key: str) -> dict:
    return {k: v for k, v in state.items() if k != key}


def como_texto(state: dict) -> str:
    untrusted = state["untrusted"]
    return (f"De: {untrusted['from_name']} <{untrusted['from_address']}>\n"
            f"Asunto: {untrusted['subject']}\n\n{untrusted['preview']}")


FICHA_VARIANTS: dict[str, Callable[[dict], dict | str]] = {
    "completa": lambda state: state,
    "sin_deterministic": lambda state: _sin(state, "deterministic"),
    "sin_checks": lambda state: _sin(state, "checks"),
    "solo_texto": lambda state: {"untrusted": state["untrusted"]},
    "texto_plano": como_texto,
}


class QuestionSet(NamedTuple):
    questions: dict
    positive: str  # opción que cuenta como «phish sí»


def _con_phish(phish: dict, impersonation: dict | None = None) -> dict:
    changes = {"phish": phish}
    if impersonation is not None:
        changes["impersonation"] = impersonation
    return {**QUESTIONS, **changes}


QUESTION_VARIANTS: dict[str, QuestionSet] = {
    "actual": QuestionSet(QUESTIONS, "sí"),
    "neutral_es": QuestionSet(_con_phish(
        {"type": "noul", "instructions": "¿Es un intento de phishing?"},
        {"type": "noul", "instructions": "¿Se hace pasar por otra marca o persona?"},
    ), "sí"),
    "neutral_en": QuestionSet(_con_phish(
        {"type": "noul", "instructions": "Is this a phishing attempt?"},
        {"type": "noul", "instructions": "Does it impersonate another brand or person?"},
    ), "sí"),
    "choice_2": QuestionSet(_con_phish({
        "type": "choice",
        "instructions": "¿Qué tipo de mensaje es?",
        "criteria": {
            "phishing": "pide datos, dinero o un clic bajo una identidad falsa.",
            "legitimo": "es lo que dice ser, sin engaño.",
        },
    }), "phishing"),
}
