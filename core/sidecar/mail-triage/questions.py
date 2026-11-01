"""Preguntas tipadas del triaje. Si se cambia una pregunta, subir QUESTIONS_ID."""

QUESTIONS_ID = "mail_triage@p1"

QUESTIONS = {
    "kind": {
        "type": "choice",
        "instructions": "¿Qué tipo de correo es?",
        "criteria": {
            "personal": "escrito por una persona conocida para una conversación personal.",
            "transaccional": "confirmación o aviso de una compra, factura, envío o cuenta que el destinatario inició.",
            "boletin_suscrito": "boletín o novedades de un servicio al que el destinatario se suscribió.",
            "marketing": "publicidad o promoción que el destinatario no pidió.",
            "estafa_phishing": "intenta engañar: suplantación, premio falso, aviso urgente falso o robo de datos.",
            "otro": "nada de lo anterior.",
        },
    },
    "phish": {
        "type": "noul",
        "instructions": "¿Intenta que el destinatario entregue credenciales, dinero o abra un enlace bajo una falsa identidad?",
    },
    "impersonation": {
        "type": "noul",
        "instructions": "¿Dice ser una marca o persona cuyo dominio no coincide con el remitente verificado?",
    },
    "urgency": {
        "type": "score",
        "instructions": "Presión de tiempo del mensaje",
        "criteria": ["sin presión", "alguna", "extrema"],
    },
    "wanted": {
        "type": "noul",
        "instructions": "Según las preferencias del destinatario, ¿querría verlo?",
    },
    "action": {
        "type": "choice",
        "instructions": "¿Qué debería hacerse con este correo?",
        "criteria": {
            "conservar": "es útil o esperado; dejarlo en la bandeja de entrada.",
            "no_deseado": "publicidad o boletín que el destinatario no querría ver; moverlo a no deseado.",
            "eliminar": "engaño claro o basura sin valor; moverlo a eliminados.",
            "revisar": "dudoso o sin certeza; que lo decida el destinatario.",
        },
    },
}
